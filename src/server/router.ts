import type { HttpBindings } from "@hono/node-server";
import { Hono } from "hono";
import { cors } from "hono/cors";
import type { ChatController } from "#src/server/controllers/ChatController.ts";
import type { ModelFitsController } from "#src/server/controllers/ModelFitsController.ts";
import type { ModelsController } from "#src/server/controllers/ModelsController.ts";

export class Router {
	readonly #app: Hono<{ Bindings: HttpBindings }>;
	readonly #modelsController: ModelsController;
	readonly #modelFitsController: ModelFitsController;
	readonly #chatController: ChatController;

	constructor(
		modelsController: ModelsController,
		modelFitsController: ModelFitsController,
		chatController: ChatController,
	) {
		this.#app = new Hono<{ Bindings: HttpBindings }>();
		this.#modelsController = modelsController;
		this.#modelFitsController = modelFitsController;
		this.#chatController = chatController;
		this.#registerRoutes();
	}

	#registerRoutes() {
		this.#app.use("/*", cors());

		this.#app.get("/v1/models", (c) => this.#modelsController.getModels(c));
		this.#app.post("/v1/chat/completions", (c) =>
			this.#chatController.getChatCompletions(c),
		);
		this.#app.get("/modelFits", (c) =>
			this.#modelFitsController.getModelFits(c),
		);
	}

	getApp(): Hono<{ Bindings: HttpBindings }> {
		return this.#app;
	}
}
