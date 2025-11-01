import type {
	ConfigRepository,
	ModelConfiguration,
} from "#src/repositories/configRepository.ts";
import type {
	EstimateParameters,
	GgufParserJson,
	GgufParserRepository,
} from "#src/repositories/ggufParserRepository.ts";
import type {
	RocmSmiQueryOptions,
	RocmSmiRepository,
	RocmSmiVramInfo,
} from "#src/repositories/rocmSmiRepository.ts";

export type ModelFitResult = {
	fits: boolean;
	requiredVramBytes: number;
	freeVramBytes: number;
	details?: string;
	message: string;
};

export class ModelNotFoundError extends Error {}

export class ModelFitService {
	readonly #ggufParserRepository: GgufParserRepository;
	readonly #rocmSmiRepository: RocmSmiRepository;
	readonly #configRepository: ConfigRepository;
	readonly #ggufCache = new Map<string, GgufParserJson>();

	constructor(
		ggufParserRepository: GgufParserRepository,
		rocmSmiRepository: RocmSmiRepository,
		configRepository: ConfigRepository,
	) {
		this.#ggufParserRepository = ggufParserRepository;
		this.#rocmSmiRepository = rocmSmiRepository;
		this.#configRepository = configRepository;
	}

	public async willModelFit(
		modelName: string,
		deviceIndex: number = 0,
	): Promise<ModelFitResult> {
		const ggufJson = await this.#getOrCacheGgufJson(modelName);
		const requiredVramBytes = this.#extractRequiredVram(ggufJson);
		const freeVramBytes = await this.#getFreeVram(deviceIndex);

		const fits = requiredVramBytes <= freeVramBytes;
		const message = fits
			? "✅ Model fits in the available VRAM."
			: "❌ Model does NOT fit in the available VRAM.";

		const details = await this.#buildDetails(deviceIndex, freeVramBytes);

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
		if (!fresh) {
			throw new Error(
				`gguf‑parser returned an empty response for "${modelName}"`,
			);
		}

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
				"gguf‑parser returned no VRAM info for the first estimate item",
			);
		}

		return firstVramInfo.nonuma;
	}

	async #getFreeVram(deviceIndex: number): Promise<number> {
		const rocmOpts: RocmSmiQueryOptions = { device: deviceIndex };
		const vramInfos: RocmSmiVramInfo[] =
			await this.#rocmSmiRepository.getVramInfo(rocmOpts);

		if (vramInfos.length === 0) {
			throw new Error(
				`rocm‑smi did not return any VRAM info for device ${deviceIndex}`,
			);
		}

		return vramInfos[0].totalBytes - vramInfos[0].usedBytes;
	}

	async #buildDetails(
		deviceIndex: number,
		totalBytes: number,
	): Promise<string> {
		const rocmOpts: RocmSmiQueryOptions = { device: deviceIndex };
		const vramInfos = await this.#rocmSmiRepository.getVramInfo(rocmOpts);
		const gpuInfo = vramInfos[0];

		return `GPU ${gpuInfo.card}: ${totalBytes.toLocaleString()} B total`;
	}
}
