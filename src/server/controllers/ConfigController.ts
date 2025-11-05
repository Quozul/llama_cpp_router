import type { Context } from "hono";

import type { ConfigService } from "#src/services/configService.ts";

export class ConfigController {
	readonly #configService: ConfigService;

	constructor(configService: ConfigService) {
		this.#configService = configService;
	}

	getConfig(c: Context) {
		return c.json(this.#configService.getConfig());
	}
}
