import { BaseCliCommandRepository } from "#src/repositories/baseCliCommandRepository.ts";

export type CacheType =
	| "f32"
	| "f16"
	| "bf16"
	| "q8_0"
	| "q4_0"
	| "q4_1"
	| "iq4_nl"
	| "q5_0"
	| "q5_1";

/** Input parameters for the estimator (public API) */
export type EstimateParameters = {
	/** Path (or URL) of the main model */
	modelFilePath: string;
	/** Path (or URL) of the multimodal projector – `null` means “no projector” */
	mmprojFilePath: string | null;
	/** Prompt context size (`--ctx-size`) */
	contextSize?: number;
	/** Disable memory‑mapped loading (`--no-mmap`) */
	noMmap?: boolean;
	/** Enable flash‑attention (`--fa`) */
	flashAttention?: boolean;
	/** Cache type for Keys (`--ctk`) */
	cacheTypeK?: CacheType;
	/** Cache type for Values (`--ctv`) */
	cacheTypeV?: CacheType;
};

export type GgufParserJson = {
	estimate: EstimatePayload;
};

export type EstimatePayload = {
	items: EstimateItem[];
	type: string;
	architecture: string;
	contextSize: number;
	flashAttention: boolean;
	noMMap: boolean;
	embeddingOnly: boolean;
	reranking: boolean;
	distributable: boolean;
	logicalBatchSize: number;
	physicalBatchSize: number;
};

export type EstimateItem = {
	offloadLayers: number;
	fullOffloaded: boolean;
	ram: MemoryInfo;
	vrams: MemoryInfo[];
};

export type MemoryInfo = {
	handleLayers: number;
	handleLastLayer: number;
	handleOutputLayer: boolean;
	remote: boolean;
	position: number;
	uma: number;
	nonuma: number;
};

export class GgufParserError extends Error {
	public readonly command: string;
	public readonly stderr: string;

	constructor(message: string, command: string, stderr: string) {
		super(message);
		this.name = "GgufParserError";
		this.command = command;
		this.stderr = stderr;
	}
}

export class GgufParserRepository extends BaseCliCommandRepository {
	public async getMemoryEstimate(
		params: EstimateParameters,
	): Promise<GgufParserJson> {
		const args = this.#buildArgs(params);
		const commandStr = `${this.binaryPath} ${args.map(this.escapeArg).join(" ")}`;

		const { stdout, stderr, exitCode } = await this.spawnAsync(args);

		if (exitCode !== 0) {
			throw new GgufParserError(
				`gguf-parser exited with code ${exitCode}`,
				commandStr,
				stderr,
			);
		}

		try {
			return JSON.parse(stdout) as GgufParserJson;
		} catch (e) {
			throw new GgufParserError(
				`Failed to parse JSON output from gguf-parser: ${(e as Error).message}`,
				commandStr,
				stdout,
			);
		}
	}

	#buildArgs(params: EstimateParameters): string[] {
		const {
			modelFilePath,
			mmprojFilePath,
			contextSize,
			noMmap = false,
			flashAttention = false,
			cacheTypeK = "f16",
			cacheTypeV = "f16",
		} = params;

		const args: string[] = [];

		args.push("--model", modelFilePath);
		if (mmprojFilePath) {
			args.push("--mmproj", mmprojFilePath);
		}

		if (contextSize !== undefined)
			args.push("--ctx-size", contextSize.toString());
		if (noMmap) args.push("--no-mmap");
		if (flashAttention) args.push("--fa");

		args.push("--ctk", cacheTypeK);
		args.push("--ctv", cacheTypeV);

		args.push(
			"--json",
			"--skip-architecture",
			"--skip-metadata",
			"--skip-tokenizer",
		);

		return args;
	}
}
