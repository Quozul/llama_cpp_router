import type { HttpBindings } from "@hono/node-server";
import type { Context } from "hono";
import { BaseCompletionController } from "#src/server/controllers/BaseCompletionController.ts";

export class OpenAiChatCompletionController extends BaseCompletionController {
	async getOpenAiChatCompletions(c: Context<{ Bindings: HttpBindings }>) {
		return this.handleCompletionRequest(c, async (model, signal, payload) => {
			return this.llamaProxyService.openAiChatCompletion(
				model,
				signal,
				payload,
			);
		});
	}
}
