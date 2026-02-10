import type { HttpBindings } from "@hono/node-server";
import { Hono } from "hono";
import { cors } from "hono/cors";
import type { CompletionController } from "#src/server/controllers/CompletionController.ts";
import type { ConfigController } from "#src/server/controllers/ConfigController.ts";
import type { EmbeddingsController } from "#src/server/controllers/EmbeddingsController.ts";
import type { ModelFitsController } from "#src/server/controllers/ModelFitsController.ts";
import type { ModelsController } from "#src/server/controllers/ModelsController.ts";
import type { OpenAiChatCompletionController } from "#src/server/controllers/OpenAiChatCompletionController.ts";
import type { OpenAiResponsesController } from "#src/server/controllers/OpenAiResponsesController.ts";

export class Router {
	readonly #app: Hono<{ Bindings: HttpBindings }>;
	readonly #modelsController: ModelsController;
	readonly #modelFitsController: ModelFitsController;
	readonly #configController: ConfigController;
	readonly #completionController: CompletionController;
	readonly #openAiResponsesController1: OpenAiResponsesController;
	readonly #openAiChatCompletionController: OpenAiChatCompletionController;
	readonly #embeddingsController: EmbeddingsController;

	constructor(
		modelsController: ModelsController,
		modelFitsController: ModelFitsController,
		completionController: CompletionController,
		openAiResponsesController: OpenAiResponsesController,
		openAiChatCompletionController: OpenAiChatCompletionController,
		embeddingsController: EmbeddingsController,
		configController: ConfigController,
	) {
		this.#app = new Hono<{ Bindings: HttpBindings }>();
		this.#modelsController = modelsController;
		this.#modelFitsController = modelFitsController;
		this.#completionController = completionController;
		this.#openAiResponsesController1 = openAiResponsesController;
		this.#openAiChatCompletionController = openAiChatCompletionController;
		this.#embeddingsController = embeddingsController;
		this.#configController = configController;
		this.#registerRoutes();
	}

	#registerRoutes() {
		this.#app.use("/*", cors());

		this.#app.get("/v1/models", (c) =>
			this.#modelsController.getOpenAiModels(c),
		);
		this.#app.post("/v1/chat/completions", (c) =>
			this.#openAiChatCompletionController.getOpenAiChatCompletions(c),
		);
		this.#app.post("/completion", (c) =>
			this.#completionController.getCompletion(c),
		);
		this.#app.post("/v1/responses", (c) =>
			this.#openAiResponsesController1.getOpenAiResponses(c),
		);
		this.#app.post("/v1/embeddings", (c) =>
			this.#embeddingsController.getOpenAiEmbeddings(c),
		);
		this.#app.get("/modelFits", (c) =>
			this.#modelFitsController.getModelFits(c),
		);
		this.#app.get("/config", (c) => this.#configController.getConfig(c));
		this.#app.post("/config", (c) => this.#configController.uploadConfig(c));
	}

	getApp(): Hono<{ Bindings: HttpBindings }> {
		return this.#app;
	}
}
