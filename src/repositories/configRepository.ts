import { readFile } from "node:fs/promises";
import { z } from "zod";
import { ZodIssueCode } from "zod/v3";

const CacheTypeSchema = z.enum([
	"f32",
	"f16",
	"bf16",
	"q8_0",
	"q4_0",
	"q4_1",
	"iq4_nl",
	"q5_0",
	"q5_1",
]);

function parseKString(v: unknown): unknown {
	if (typeof v !== "string") return v;

	const trimmed = v.trim().toLowerCase();

	if (trimmed.endsWith("k")) {
		const numPart = trimmed.slice(0, -1);
		const parsed = Number(numPart);
		if (!Number.isNaN(parsed)) return parsed * 1024;
	}

	const asNumber = Number(trimmed);
	if (!Number.isNaN(asNumber)) return asNumber;

	return v;
}

const ContextSizeSchema = z
	.preprocess(parseKString, z.number().int().nonnegative())
	.default(4096);

const CommonSchema = z.object({
	cacheType: CacheTypeSchema.default("q8_0"),
	contextSize: ContextSizeSchema,
	noMmap: z.boolean().default(true),
	flashAttention: z.boolean().default(true),
	jinja: z.boolean().default(true),
});

const SamplingSchema = z.object({
	temperature: z.number().positive().default(0.8),
	topK: z.number().int().nonnegative().default(40),
	topP: z.number().min(0).max(1).default(0.9),
	minP: z.number().min(0).max(1).default(0.1),
	repeatPenalty: z.number().positive().default(1.0),
	mirostat: z.union([z.literal(0), z.literal(1), z.literal(2)]).default(2),
});

const NetworkSchema = z.object({
	host: z.string().default("127.0.0.1"),
	port: z.number().int().positive(),
});

const ModelConfigurationSchema = z.object({
	modelFilePath: z.string(),
	multimodalProjectorFilePath: z.string().nullable().default(null),
	unloadable: z.boolean().default(true),
	common: CommonSchema,
	sampling: SamplingSchema,
	network: NetworkSchema,
});

const SystemConfigurationSchema = z.object({
	llamaServer: z.string(),
	ggufParser: z.string(),
	rocmSmi: z.string(),
});

const ServerConfigurationSchema = z.object({
	hostname: z.string().default("0.0.0.0"),
	port: z.number().default(8080),
});

const ConfigFileSchema = z
	.object({
		owner: z.string(),
		system: SystemConfigurationSchema,
		server: ServerConfigurationSchema,
		models: z.record(z.string(), ModelConfigurationSchema),
	})
	.superRefine((data, ctx) => {
		const seen = new Map<string, string>();

		for (const [modelName, cfg] of Object.entries(data.models)) {
			const key = `${cfg.network.host}:${cfg.network.port}`;

			const firstModel = seen.get(key);
			if (firstModel !== undefined) {
				ctx.addIssue({
					code: ZodIssueCode.custom,
					message: `Network host/port "${key}" is already used by model "${firstModel}"`,
					path: ["models", modelName, "network"],
				});
				ctx.addIssue({
					code: ZodIssueCode.custom,
					message: `Network host/port "${key}" collides with model "${modelName}"`,
					path: ["models", firstModel, "network"],
				});
			} else {
				seen.set(key, modelName);
			}
		}
	});

export type ModelConfiguration = z.infer<typeof ModelConfigurationSchema>;

export type ConfigFile = z.infer<typeof ConfigFileSchema>;

export type SystemConfiguration = z.infer<typeof SystemConfigurationSchema>;

export type ServerConfiguration = z.infer<typeof ServerConfigurationSchema>;

export class ConfigRepository {
	readonly #config: ConfigFile;

	private constructor(config: ConfigFile) {
		this.#config = config;
	}

	public static async createFromFile(
		configPath: string = "./config.json",
	): Promise<ConfigRepository> {
		const raw = await readFile(configPath, { encoding: "utf8" });
		let json: unknown;
		try {
			json = JSON.parse(raw);
		} catch (err) {
			throw new Error(
				`Failed to parse JSON config at "${configPath}": ${(err as Error).message}`,
			);
		}

		const parsed = ConfigFileSchema.safeParse(json);
		if (!parsed.success) {
			const issues = parsed.error.issues
				.map((i) => `${i.path.join(".")}: ${i.message}`)
				.join("\n");
			throw new Error(`Config validation error in "${configPath}":\n${issues}`);
		}

		return new ConfigRepository(parsed.data);
	}

	public getSystemConfiguration(): SystemConfiguration {
		return this.#config.system;
	}

	public getServerConfiguration(): ServerConfiguration {
		return this.#config.server;
	}

	public getAvailableModelNames(): string[] {
		return Object.keys(this.#config.models);
	}

	public getModelOwnerName(): string {
		return this.#config.owner;
	}

	public getModelConfiguration(modelName: string): ModelConfiguration | null {
		const cfg = this.#config.models[modelName];
		return cfg ?? null;
	}
}
