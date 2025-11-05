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

	async uploadConfig(c: Context) {
		try {
			const body = await c.req.json();
			await this.#configService.reloadConfig(body);
			return c.json({
				success: true,
				message: "Config reloaded successfully",
			});
		} catch (error) {
			return c.json(
				{
					success: false,
					error: (error as Error).message,
				},
				400,
			);
		}
	}
}
