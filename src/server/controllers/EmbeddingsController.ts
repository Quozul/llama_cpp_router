import type { HttpBindings } from "@hono/node-server";
import type { Context } from "hono";
import { HTTPException } from "hono/http-exception";
import type { LlamaProxyService } from "#src/services/llamaProxyService.ts";

export class EmbeddingsController {
	readonly #llamaProxyService: LlamaProxyService;

	constructor(llamaProxyService: LlamaProxyService) {
		this.#llamaProxyService = llamaProxyService;
	}

	async getEmbeddings(c: Context<{ Bindings: HttpBindings }>) {
		const request = await c.req.json();
		if ("model" in request) {
			const model = request.model;
			const abortController = new AbortController();
			c.header("Content-Type", "application/json");
			c.env.outgoing.on("close", () => {
				abortController.abort();
			});
			const response = await this.#proxy(
				model,
				abortController.signal,
				request,
			);
			return c.body(response);
		}
	}

	async #proxy(
		model: string,
		abortSignal: AbortSignal,
		request: unknown,
	): Promise<ReadableStream<Uint8Array<ArrayBuffer>>> {
		const response = await this.#llamaProxyService.embeddings(
			model,
			abortSignal,
			JSON.stringify(request),
		);
		if (!response) {
			throw new HTTPException(500);
		}
		return response;
	}
}
