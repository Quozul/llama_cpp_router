import { type HttpBindings, serve } from "@hono/node-server";
import type { Hono } from "hono";
import { HTTPException } from "hono/http-exception";
import { logger } from "hono/logger";
import type { ConfigRepository } from "#src/repositories/configRepository.ts";

export class Server {
	readonly #app: Hono<{ Bindings: HttpBindings }>;
	readonly #configRepository: ConfigRepository;

	constructor(
		app: Hono<{ Bindings: HttpBindings }>,
		configRepository: ConfigRepository,
	) {
		this.#app = app;
		this.#configRepository = configRepository;
		this.#setupMiddleware();
		this.#setupErrorHandling();
	}

	#setupMiddleware() {
		// Add logging middleware
		this.#app.use("*", logger());
	}

	#setupErrorHandling() {
		// Global error handler
		this.#app.onError((err, c) => {
			console.error(`[${c.req.method}] ${c.req.url} - ${err.message}`);

			if (err instanceof HTTPException) {
				return c.json(
					{
						error: err.message,
					},
					err.status,
				);
			}

			return c.json(
				{
					error: err.message,
				},
				500,
			);
		});

		// 404 handler
		this.#app.notFound((c) => {
			return c.json(
				{
					notFound: true,
				},
				404,
			);
		});
	}

	run() {
		const { port, hostname } = this.#configRepository.getServerConfiguration();
		serve(
			{
				fetch: this.#app.fetch,
				hostname,
				port,
			},
			(info) => {
				const address = `http://${info.address}:${info.port}`;
				console.log("🌐 Server listening on", address);
			},
		);
	}
}
