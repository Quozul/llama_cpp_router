import type { ConfigRepository } from "#src/repositories/configRepository.ts";
import type { LlamaServerRepository } from "#src/repositories/llamaServerRepository.ts";
import {
	type ModelFitService,
	ModelNotFoundError,
} from "#src/services/modelFitService.ts";

export class InsufficientMemoryError extends Error {}

export class LlamaProxyService {
	readonly #configRepository: ConfigRepository;
	readonly #llamaServerRepository: LlamaServerRepository;
	readonly #modelFitService: ModelFitService;

	readonly #models = new Map<string, number>();
	readonly #ongoingRequests = new Set<string>();
	readonly #lastUsed = new Map<string, number>();

	constructor(
		configRepository: ConfigRepository,
		llamaServerRepository: LlamaServerRepository,
		modelFitService: ModelFitService,
	) {
		this.#configRepository = configRepository;
		this.#llamaServerRepository = llamaServerRepository;
		this.#modelFitService = modelFitService;
	}

	public async proxyRequest(
		modelName: string,
		abortSignal: AbortSignal,
		body?: BodyInit | null,
	): Promise<ReadableStream<Uint8Array<ArrayBuffer>> | null> {
		this.#ongoingRequests.add(modelName);
		return this.#forwardRequest(modelName, abortSignal, body).finally(() => {
			this.#ongoingRequests.delete(modelName);
		});
	}

	async #forwardRequest(
		modelName: string,
		abortSignal: AbortSignal,
		body?: BodyInit | null,
	): Promise<ReadableStream<Uint8Array<ArrayBuffer>> | null> {
		const modelConfig = this.#configRepository.getModelConfiguration(modelName);
		if (!modelConfig) {
			throw new ModelNotFoundError(
				"modelConfig is missing a valid configuration object",
			);
		}

		if (!this.#models.has(modelName)) {
			let fitResult = await this.#modelFitService.willModelFit(modelName);
			if (!fitResult.fits) {
				const unloadableCandidates = Array.from(this.#models.keys())
					.map((name) => ({
						name,
						lastUsed: this.#lastUsed.get(name) ?? 0,
						config: this.#configRepository.getModelConfiguration(name),
					}))
					.filter(
						(candidate) =>
							candidate.config?.unloadable !== false &&
							!this.#ongoingRequests.has(candidate.name),
					)
					.sort((a, b) => a.lastUsed - b.lastUsed);

				for (const candidate of unloadableCandidates) {
					const pidToStop = this.#models.get(candidate.name);
					if (pidToStop) {
						console.log(`Unloading ${candidate.name}`);
						await this.#llamaServerRepository.stop(pidToStop);
						this.#models.delete(candidate.name);
						this.#lastUsed.delete(candidate.name);
						fitResult = await this.#modelFitService.willModelFit(modelName);
						if (fitResult.fits) {
							break;
						}
					}
				}
			}

			if (!fitResult.fits) {
				throw new InsufficientMemoryError(
					`${modelName} needs ${fitResult.requiredVramBytes} B but only ${fitResult.freeVramBytes} B available after attempting to unload other models`,
				);
			}

			console.log(`Loading ${modelName}`);
			const llamaServerHandle =
				await this.#llamaServerRepository.start(modelConfig);
			this.#models.set(modelName, llamaServerHandle.pid);
		}

		this.#lastUsed.set(modelName, Date.now());

		const originServer = `http://${modelConfig.network.host}:${modelConfig.network.port}`;
		const response = await fetch(`${originServer}/v1/chat/completions`, {
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				Accept: "application/json",
			},
			signal: abortSignal,
			body,
		});
		return response.body;
	}
}
