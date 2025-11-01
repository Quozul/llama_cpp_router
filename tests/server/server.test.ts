import assert from "node:assert";
import { mock, test } from "node:test";
import type { ConfigRepository } from "#src/repositories/configRepository.ts";
import { ChatController } from "#src/server/controllers/ChatController.ts";
import { ModelFitsController } from "#src/server/controllers/ModelFitsController.ts";
import { ModelsController } from "#src/server/controllers/ModelsController.ts";
import { Router } from "#src/server/router.ts";
import { Server } from "#src/server/server.ts";
import type { LlamaProxyService } from "#src/services/llamaProxyService.ts";
import type { ModelFitService } from "#src/services/modelFitService.ts";
import { Model, type ModelsService } from "#src/services/modelsService.ts";

function mockRouter(owner: string = "", ...models: string[]) {
	const modelService = {
		getModels: mock.fn(() => models.map((id) => new Model(id, owner))),
	};
	const modelFitService = {} as ModelFitService;
	const llamaProxyService = {} as LlamaProxyService;

	const modelsController = new ModelsController(
		modelService as unknown as ModelsService,
	);
	const modelFitsController = new ModelFitsController(modelFitService);
	const chatController = new ChatController(llamaProxyService);

	const router = new Router(
		modelsController,
		modelFitsController,
		chatController,
	);
	return { router, modelService };
}

test("handleRequest", async (t) => {
	await t.test("should handle root path", async () => {
		// Arrange
		const { router } = mockRouter();
		const app = router.getApp();
		const configRepository = {
			getServerConfiguration: mock.fn(() => ({
				port: process.env.PORT,
				host: process.env.HOST,
			})),
		} as unknown as ConfigRepository;
		new Server(app, configRepository);

		// Act
		const res = await app.request("/", { method: "GET" });

		// Assert
		assert.strictEqual(res.status, 404);
		const body = await res.json();
		assert.deepStrictEqual(body, { notFound: true });
	});
});

test("GET /v1/models", async (t) => {
	await t.test("should return model list", async () => {
		// Arrange
		const givenOwner = "bob";
		const givenModels = ["model-a", "model-b"];
		const { router, modelService } = mockRouter(givenOwner, ...givenModels);
		const app = router.getApp();
		const expectedResponse = {
			object: "list",
			data: [
				{
					id: "model-a",
					object: "model",
					owned_by: "bob",
				},
				{
					id: "model-b",
					object: "model",
					owned_by: "bob",
				},
			],
		};

		// Act
		const res = await app.request("/v1/models", { method: "GET" });

		// Assert
		assert.strictEqual(res.status, 200);
		assert.strictEqual(
			modelService.getModels.mock.callCount(),
			1,
			"service should be called once",
		);
		const body = await res.json();
		assert.partialDeepStrictEqual(
			body,
			expectedResponse,
			"response does not include expected fields",
		);
	});

	await t.test(
		"should return method not allowed when post request",
		async () => {
			// Arrange
			const { router, modelService } = mockRouter();
			const app = router.getApp();

			// Act
			const res = await app.request("/v1/models", { method: "POST" });

			// Assert
			assert.strictEqual(res.status, 404);
			assert.strictEqual(
				modelService.getModels.mock.callCount(),
				0,
				"service should not be called",
			);
		},
	);
});
