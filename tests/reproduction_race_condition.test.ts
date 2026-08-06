import assert from "node:assert";
import { mock, test } from "node:test";
import type { ConfigRepository } from "#src/repositories/configRepository.ts";
import type { LlamaServerRepository } from "#src/repositories/llamaServerRepository.ts";
import { LlamaProxyService } from "#src/services/llamaProxyService.ts";
import type { ModelFitService } from "#src/services/modelFitService.ts";

test("LlamaProxyService race condition reproduction", async () => {
	// Arrange
	const configRepository = {
		getModelConfiguration: mock.fn(() => ({
			modelFilePath: "/tmp/model.gguf",
			network: { host: "localhost", port: 8080 },
			common: {},
			sampling: {},
		})),
		getConcurrentModels: mock.fn(() => 5),
		getModelUnloadDuration: mock.fn(() => 5),
	};

	const llamaServerRepository = {
		start: mock.fn(async () => {
			await new Promise((resolve) => setTimeout(resolve, 50)); // Simulate startup delay
			return { pid: 123 };
		}),
		onProcessCrash: mock.fn(),
		stop: mock.fn(),
	};

	const modelFitService = {
		willModelFit: mock.fn(async () => ({ fits: true })),
	};

	const service = new LlamaProxyService(
		configRepository as unknown as ConfigRepository,
		llamaServerRepository as unknown as LlamaServerRepository,
		modelFitService as unknown as ModelFitService,
	);

	// Mock global fetch to avoid actual network calls
	const originalFetch = global.fetch;
	global.fetch = mock.fn(async () => new Response("ok"));

	try {
		// Act
		const req1 = service.completion("model-1", new AbortController().signal);
		const req2 = service.completion("model-1", new AbortController().signal);

		await Promise.all([req1, req2]);

		// Assert
		const startCallCount = llamaServerRepository.start.mock.callCount();
		assert.strictEqual(
			startCallCount,
			1,
			"start() should be called exactly once for concurrent requests",
		);
	} finally {
		global.fetch = originalFetch;
	}
});
