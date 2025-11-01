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
			const abortSignal = new AbortController();
			if (isStreamingRequest) {
				c.header("Content-Type", "text/event-stream");
				return stream(c, async (stream) => {
					const interval = setInterval(() => {
						stream.write(": model is loading\r\n\r\n");
					}, 1_000);
					stream.onAbort(() => {
						abortSignal.abort();
						clearInterval(interval);
					});
					const response = await this.#proxy(
						model,
						abortSignal.signal,
						request,
					);
					clearInterval(interval);
					await stream.pipe(response);
				});
			} else {
				c.env.outgoing.on("close", () => {
					abortSignal.abort();
				});
				const response = await this.#proxy(model, abortSignal.signal, request);
				return c.body(response);
			}
		}
	}

	async #proxy(model: string, abortSignal: AbortSignal, request: unknown) {
		try {
			const response = await this.#llamaProxyService.proxyRequest(
				model,
				abortSignal,
				JSON.stringify(request),
			);
			if (!response) {
				throw new HTTPException(500);
			}
			return response;
		} catch (e) {
			if (e instanceof InsufficientMemoryError) {
				throw new HTTPException(500, {
					message: "Insufficient memory",
				});
			}
			throw e;
		}
	}
}
