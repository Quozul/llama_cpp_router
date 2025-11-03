import type { HttpBindings } from "@hono/node-server";
import type { Context } from "hono";
import { HTTPException } from "hono/http-exception";
import { stream } from "hono/streaming";
import {
	InsufficientMemoryError,
	type LlamaProxyService,
} from "#src/services/llamaProxyService.ts";

export class ChatController {
	readonly #llamaProxyService: LlamaProxyService;

	constructor(llamaProxyService: LlamaProxyService) {
		this.#llamaProxyService = llamaProxyService;
	}

	async getChatCompletions(c: Context<{ Bindings: HttpBindings }>) {
		const request = await c.req.json();
		if ("stream" in request && "model" in request) {
			const model = request.model;
			const isStreamingRequest = request.stream;
			const abortController = new AbortController();
			if (isStreamingRequest) {
				return this.#stream(c, model, abortController, request);
			} else {
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
	}

	async #stream(
		c: Context<{ Bindings: HttpBindings }>,
		model: string,
		abortController: AbortController,
		request: unknown,
	) {
		try {
			const response = await this.#proxy(
				model,
				abortController.signal,
				request,
			);

			c.header("Content-Type", "text/event-stream");
			return stream(c, async (stream) => {
				const interval = setInterval(() => {
					stream.write(": model is loading\r\n\r\n");
				}, 1_000);
				stream.onAbort(() => {
					abortController.abort();
					clearInterval(interval);
				});
				clearInterval(interval);
				await stream.pipe(response);
			});
		} catch (e) {
			if (e instanceof InsufficientMemoryError) {
				throw new HTTPException(500, { message: "Insufficient memory" });
			} else {
				throw e;
			}
		}
	}

	async #proxy(
		model: string,
		abortSignal: AbortSignal,
		request: unknown,
	): Promise<ReadableStream<Uint8Array<ArrayBuffer>>> {
		const response = await this.#llamaProxyService.chatCompletion(
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
