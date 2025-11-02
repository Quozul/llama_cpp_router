import type { ConfigRepository } from "#src/repositories/configRepository.ts";
import type { LlamaServerRepository } from "#src/repositories/llamaServerRepository.ts";
import {
	type ModelFitService,
	ModelNotFoundError,
} from "#src/services/modelFitService.ts";

export class InsufficientMemoryError extends Error {}
export class NotSupportedError extends Error {}

export class LlamaProxyService {
	readonly #configRepository: ConfigRepository;
	readonly #llamaServerRepository: LlamaServerRepository;
	readonly #modelFitService: ModelFitService;

	readonly #models = new Map<string, number>();
	readonly #ongoingRequests = new Set<string>();
	readonly #lastUsed = new Map<string, number>();
	readonly #unloadTimers = new Map<string, NodeJS.Timeout>(); // Track unload timers

	constructor(
		configRepository: ConfigRepository,
		llamaServerRepository: LlamaServerRepository,
		modelFitService: ModelFitService,
	) {
		this.#configRepository = configRepository;
		this.#llamaServerRepository = llamaServerRepository;
		this.#modelFitService = modelFitService;
	}

	public async chatCompletion(
		modelName: string,
		abortSignal: AbortSignal,
		body?: BodyInit | null,
	): Promise<ReadableStream<Uint8Array<ArrayBuffer>> | null> {
		this.#ongoingRequests.add(modelName);
		return this.#forwardRequest(
			modelName,
			"chat/completions",
			abortSignal,
			body,
		).finally(() => {
			this.#ongoingRequests.delete(modelName);
		});
	}

	public async embeddings(
		modelName: string,
		abortSignal: AbortSignal,
		body?: BodyInit | null,
	): Promise<ReadableStream<Uint8Array<ArrayBuffer>> | null> {
		// Ensure model supports embeddings
		const modelConfig = this.#configRepository.getModelConfiguration(modelName);
		if (!modelConfig) {
			throw new ModelNotFoundError(
				"modelConfig is missing a valid configuration object",
			);
		}
		if (!modelConfig.embeddings) {
			throw new NotSupportedError("This server does not support embeddings.");
		}

		this.#ongoingRequests.add(modelName);
		return this.#forwardRequest(
			modelName,
			"embeddings",
			abortSignal,
			body,
		).finally(() => {
			this.#ongoingRequests.delete(modelName);
		});
	}

	async #forwardRequest(
		modelName: string,
		resource: "chat/completions" | "embeddings",
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
						await this.#unloadModel(modelName);
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

		this.#resetUnloadTimer(modelName);

		this.#lastUsed.set(modelName, Date.now());

		const originServer = `http://${modelConfig.network.host}:${modelConfig.network.port}`;
		const response = await fetch(`${originServer}/v1/${resource}`, {
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

	#resetUnloadTimer(modelName: string): void {
		if (this.#unloadTimers.has(modelName)) {
			clearTimeout(this.#unloadTimers.get(modelName));
			this.#unloadTimers.delete(modelName);
		}

		const unloadMinutes = this.#configRepository.getModelUnloadDuration();
		const timeoutMs = unloadMinutes * 60 * 1000;

		const timer = setTimeout(
			this.#unloadModel.bind(this),
			timeoutMs,
			modelName,
		);

		this.#unloadTimers.set(modelName, timer);
	}

	async #unloadModel(modelName: string): Promise<void> {
		const pid = this.#models.get(modelName);
		if (pid) {
			console.log(`Unloading ${modelName}`);

			try {
				await this.#llamaServerRepository.stop(pid);
				const timer = this.#unloadTimers.get(modelName);
				if (timer) {
					clearTimeout(timer);
				}
			} catch (error) {
				console.error(`Failed to stop model ${modelName}:`, error);
			} finally {
				this.#models.delete(modelName);
				this.#unloadTimers.delete(modelName);
				this.#lastUsed.delete(modelName);
			}
		}
	}
}
