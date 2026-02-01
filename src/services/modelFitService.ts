import type {
	MemoryInfoRepository,
	MemoryUsageInfo,
} from "#src/interfaces/memoryInfoRepository.ts";
import type {
	ConfigRepository,
	ModelConfiguration,
} from "#src/repositories/configRepository.ts";
import type {
	EstimateParameters,
	GgufParserJson,
	GgufParserRepository,
} from "#src/repositories/ggufParserRepository.ts";

export type ModelFitResult = {
	fits: boolean;
	requiredVramBytes: number;
	freeVramBytes: number;
	details?: string;
	message: string;
};

export class ModelNotFoundError extends Error {}
export class NoGpuError extends Error {}

export class ModelFitService {
	readonly #ggufParserRepository: GgufParserRepository;
	readonly #memoryInfoRepository: MemoryInfoRepository;
	readonly #configRepository: ConfigRepository;
	readonly #ggufCache = new Map<string, GgufParserJson>();

	constructor(
		ggufParserRepository: GgufParserRepository,
		rocmSmiRepository: MemoryInfoRepository,
		configRepository: ConfigRepository,
	) {
		this.#ggufParserRepository = ggufParserRepository;
		this.#memoryInfoRepository = rocmSmiRepository;
		this.#configRepository = configRepository;
	}

	public async willModelFit(modelName: string): Promise<ModelFitResult> {
		const ggufJson = await this.#getOrCacheGgufJson(modelName);
		const requiredVramBytes = this.#extractRequiredVram(ggufJson);
		const freeVramBytes = await this.#getFreeVram();

		const fits = requiredVramBytes <= freeVramBytes;
		const message = fits
			? "✅ Model fits in the available memory."
			: "❌ Model does NOT fit in the available memory.";

		const details = await this.#buildDetails(freeVramBytes);

		return {
			fits,
			requiredVramBytes,
			freeVramBytes,
			details,
			message,
		};
	}

	#getModelConfigurationOrThrow(modelName: string) {
		const modelConfig = this.#configRepository.getModelConfiguration(modelName);
		if (!modelConfig) {
			throw new ModelNotFoundError(
				`Model configuration for "${modelName}" not found`,
			);
		}
		return modelConfig;
	}

	#buildEstimateParameters(
		modelConfig: ModelConfiguration,
	): EstimateParameters {
		return {
			modelFilePath: modelConfig.modelFilePath,
			mmprojFilePath: modelConfig.multimodalProjectorFilePath,
			contextSize: modelConfig.common.contextSize,
			noMmap: modelConfig.common.noMmap,
			flashAttention: modelConfig.common.flashAttention,
			cacheTypeK: modelConfig.common.cacheType,
			cacheTypeV: modelConfig.common.cacheType,
		};
	}

	async #getOrCacheGgufJson(modelName: string): Promise<GgufParserJson> {
		const cached = this.#ggufCache.get(modelName);
		if (cached) {
			return cached;
		}

		const modelConfig = this.#getModelConfigurationOrThrow(modelName);
		const ggufParams = this.#buildEstimateParameters(modelConfig);
		const fresh =
			await this.#ggufParserRepository.getMemoryEstimate(ggufParams);

		this.#ggufCache.set(modelName, fresh);
		return fresh;
	}

	#extractRequiredVram(ggufJson: GgufParserJson): number {
		const firstItem = ggufJson.estimate.items[0];
		if (!firstItem) {
			throw new Error("gguf‑parser returned no estimate items");
		}

		const firstVramInfo = firstItem.vrams[0];
		if (!firstVramInfo) {
			throw new Error(
				"gguf‑parser returned no memory info for the first estimate item",
			);
		}

		return firstVramInfo.nonuma;
	}

	async #getFreeVram(): Promise<number> {
		const gpuInfo = await this.#getFirstGpu();
		return gpuInfo.totalBytes - gpuInfo.usedBytes;
	}

	async #buildDetails(totalBytes: number): Promise<string> {
		const gpuInfo = await this.#getFirstGpu();
		return `GPU ${gpuInfo.sourceId}: ${totalBytes.toLocaleString()} B total`;
	}

	async #getFirstGpu(): Promise<MemoryUsageInfo> {
		const hasAtLeastOneElement = <T>(arr: T[]): arr is [T] => arr.length > 0;
		const vramInfos = await this.#memoryInfoRepository.getMemoryInfo();
		if (hasAtLeastOneElement(vramInfos)) {
			return vramInfos[0];
		}
		throw new NoGpuError();
	}
}
