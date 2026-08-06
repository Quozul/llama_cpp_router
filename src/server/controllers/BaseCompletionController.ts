import type { HttpBindings } from "@hono/node-server";
import type { Context } from "hono";
import { HTTPException } from "hono/http-exception";
import { stream } from "hono/streaming";
import { hasModelField, isStreamRequest } from "#src/common/streamDetection.ts";
import {
	InsufficientMemoryError,
	type LlamaProxyService,
} from "#src/services/llamaProxyService.ts";

export abstract class BaseCompletionController {
	readonly #llamaProxyService: LlamaProxyService;

	constructor(llamaProxyService: LlamaProxyService) {
		this.#llamaProxyService = llamaProxyService;
	}

	protected get llamaProxyService() {
		return this.#llamaProxyService;
	}

	protected async handleCompletionRequest(
		c: Context<{ Bindings: HttpBindings }>,
		proxyCall: (
			model: string,
			signal: AbortSignal,
			payload: string,
		) => Promise<ReadableStream<Uint8Array<ArrayBuffer>> | null>,
	) {
		const request = await c.req.json();
		if (hasModelField(request)) {
			const isStreamingRequest = isStreamRequest(request);
			const abortController = new AbortController();
			const payload = JSON.stringify(request);

			if (isStreamingRequest) {
				return this.#stream(
					c,
					request.model,
					abortController,
					payload,
					proxyCall,
				);
			} else {
				c.header("Content-Type", "application/json");
				c.env.outgoing.on("close", () => {
					abortController.abort();
				});
				const response = await this.#proxy(
					request.model,
					abortController.signal,
					payload,
					proxyCall,
				);
				return c.body(response);
			}
		}
	}

	async #stream(
		c: Context<{ Bindings: HttpBindings }>,
		model: string,
		abortController: AbortController,
		payload: string,
		proxyCall: (
			model: string,
			signal: AbortSignal,
			payload: string,
		) => Promise<ReadableStream<Uint8Array<ArrayBuffer>> | null>,
	) {
		try {
			const response = await this.#proxy(
				model,
				abortController.signal,
				payload,
				proxyCall,
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
		payload: string,
		proxyCall: (
			model: string,
			signal: AbortSignal,
			payload: string,
		) => Promise<ReadableStream<Uint8Array<ArrayBuffer>> | null>,
	): Promise<ReadableStream<Uint8Array<ArrayBuffer>>> {
		const response = await proxyCall(model, abortSignal, payload);
		if (!response) {
			throw new HTTPException(500);
		}
		return response;
	}
}
