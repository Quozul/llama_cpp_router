import type { Context } from "hono";
import type { ModelsService } from "#src/services/modelsService.ts";

export class ModelsController {
	readonly #modelService: ModelsService;

	constructor(modelService: ModelsService) {
		this.#modelService = modelService;
	}

	async getModels(c: Context) {
		const models = this.#modelService.getModels();
		return c.json({
			object: "list",
			data: models.map((model) => ({
				object: "model",
				id: model.id,
				createdAt: Date.now(),
				owned_by: model.owner,
			})),
		});
	}
}
