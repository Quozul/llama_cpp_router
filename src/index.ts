import type { MemoryInfoRepository } from "#src/interfaces/memoryInfoRepository.ts";
import { ConfigRepository } from "#src/repositories/configRepository.ts";
import { GgufParserRepository } from "#src/repositories/ggufParserRepository.ts";
import { LlamaServerRepository } from "#src/repositories/llamaServerRepository.ts";
import { RocmSmiRepository } from "#src/repositories/rocmSmiRepository.ts";
import { SystemMemoryRepository } from "#src/repositories/systemMemoryRepository.ts";
import { CompletionController } from "#src/server/controllers/CompletionController.ts";
import { ConfigController } from "#src/server/controllers/ConfigController.ts";
import { EmbeddingsController } from "#src/server/controllers/EmbeddingsController.ts";
import { ModelFitsController } from "#src/server/controllers/ModelFitsController.ts";
import { ModelsController } from "#src/server/controllers/ModelsController.ts";
import { OpenAiChatCompletionController } from "#src/server/controllers/OpenAiChatCompletionController.ts";
import { OpenAiResponsesController } from "#src/server/controllers/OpenAiResponsesController.ts";
import { Router } from "#src/server/router.ts";
import { Server } from "#src/server/server.ts";
import { ConfigService } from "#src/services/configService.ts";
import { LlamaProxyService } from "#src/services/llamaProxyService.ts";
import { ModelFitService } from "#src/services/modelFitService.ts";
import { ModelsService } from "#src/services/modelsService.ts";

if (import.meta.main) {
	// Repositories
	let configPath = "./config.json";
	if (process.argv.length > 2) {
		configPath = process.argv.slice(2).join(" ");
	}
	const configRepository = await ConfigRepository.createFromFile(
		configPath,
	).catch((err) => {
		console.error(err);
		process.exit(1);
	});
	const llamaServerRepository = new LlamaServerRepository(
		configRepository.getSystemConfiguration().llamaServer,
	);
	const ggufParserRepository = new GgufParserRepository(
		configRepository.getSystemConfiguration().ggufParser,
	);

	const memoryType = configRepository.getMemoryType();
	let memoryInfoRepository: MemoryInfoRepository;
	if (memoryType === "unified") {
		memoryInfoRepository = new SystemMemoryRepository();
	} else if (memoryType === "amd") {
		memoryInfoRepository = new RocmSmiRepository(
			configRepository.getSystemConfiguration().rocmSmi,
		);
	} else {
		throw new Error("Unsupported memory type");
	}

	// Services
	const modelFitService = new ModelFitService(
		ggufParserRepository,
		memoryInfoRepository,
		configRepository,
	);
	const modelService = new ModelsService(configRepository);
	const llamaProxyService = new LlamaProxyService(
		configRepository,
		llamaServerRepository,
		modelFitService,
	);

	// Controllers
	const modelsController = new ModelsController(modelService);
	const modelFitsController = new ModelFitsController(modelFitService);
	const completionController = new CompletionController(llamaProxyService);
	const responsesController = new OpenAiResponsesController(llamaProxyService);
	const openAiChatCompletionController = new OpenAiChatCompletionController(
		llamaProxyService,
	);
	const embeddingsController = new EmbeddingsController(llamaProxyService);

	// Router and Server
	const configService = new ConfigService(configRepository);
	const configController = new ConfigController(configService);
	const router = new Router(
		modelsController,
		modelFitsController,
		completionController,
		responsesController,
		openAiChatCompletionController,
		embeddingsController,
		configController,
	);
	new Server(router.getApp(), configRepository).run();
}
