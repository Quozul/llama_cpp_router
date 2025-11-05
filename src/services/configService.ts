import type { ConfigRepository } from "#src/repositories/configRepository.ts";

export class ConfigService {
	readonly #configRepository: ConfigRepository;

	constructor(configRepository: ConfigRepository) {
		this.#configRepository = configRepository;
	}

	public getConfig() {
		return this.#configRepository.getConfig();
	}
}
