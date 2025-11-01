import type { Context } from "hono";
import { HTTPException } from "hono/http-exception";
import {
	type ModelFitService,
	ModelNotFoundError,
} from "#src/services/modelFitService.ts";

export class ModelFitsController {
	readonly #modelFitService: ModelFitService;

	constructor(modelFitService: ModelFitService) {
		this.#modelFitService = modelFitService;
	}

	async getModelFits(c: Context) {
		const model = c.req.query("model");
		if (!model) {
			throw new HTTPException(400, { message: "bad request" });
		}

		try {
			const estimate = await this.#modelFitService.willModelFit(model);
			return c.json(estimate);
		} catch (e) {
			if (e instanceof ModelNotFoundError) {
				throw new HTTPException(404, { message: "not found" });
			}
			throw e;
		}
	}
}
