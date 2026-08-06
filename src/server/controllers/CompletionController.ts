import type { HttpBindings } from "@hono/node-server";
import type { Context } from "hono";
import { BaseCompletionController } from "#src/server/controllers/BaseCompletionController.ts";

export class CompletionController extends BaseCompletionController {
	async getCompletion(c: Context<{ Bindings: HttpBindings }>) {
		return this.handleCompletionRequest(c, async (model, signal, payload) => {
			return this.llamaProxyService.completion(model, signal, payload);
		});
	}
}
