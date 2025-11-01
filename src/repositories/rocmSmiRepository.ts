import { BaseCliCommandRepository } from "#src/repositories/baseCliCommandRepository.ts";

export type RocmSmiVramInfo = {
	card: string;
	totalBytes: number;
	usedBytes: number;
};

export type RocmSmiRawResult = Record<
	string,
	{
		"VRAM Total Memory (B)"?: string;
		"VRAM Total Used Memory (B)"?: string;
	}
>;

export type RocmSmiQueryOptions = {
	device?: number;
};

export class RocmSmiError extends Error {
	public readonly command: string;
	public readonly stderr: string;

	constructor(message: string, command: string, stderr: string) {
		super(message);
		this.name = "RocmSmiError";
		this.command = command;
		this.stderr = stderr;
	}
}

export class RocmSmiRepository extends BaseCliCommandRepository {
	public async getVramInfo(
		opts: RocmSmiQueryOptions = {},
	): Promise<RocmSmiVramInfo[]> {
		const args = this.#buildArgs(opts);
		const commandStr = `${this.binaryPath} ${args.map(this.escapeArg).join(" ")}`;

		const { stdout, stderr, exitCode } = await this.spawnAsync(args);

		if (exitCode !== 0) {
			throw new RocmSmiError(
				`rocm-smi exited with error code ${exitCode}`,
				commandStr,
				stderr,
			);
		}

		let raw: RocmSmiRawResult;
		try {
			raw = JSON.parse(stdout) as RocmSmiRawResult;
		} catch (e) {
			throw new RocmSmiError(
				`Failed to parse JSON output from rocm-smi: ${(e as Error).message}`,
				commandStr,
				stdout,
			);
		}

		return Object.entries(raw).map(([card, data]) => {
			const totalStr = data["VRAM Total Memory (B)"];
			const usedStr = data["VRAM Total Used Memory (B)"];
			const total = totalStr ? Number(totalStr) : NaN;
			const used = usedStr ? Number(usedStr) : NaN;

			return {
				card,
				totalBytes: total,
				usedBytes: used,
			};
		});
	}

	#buildArgs(opts: RocmSmiQueryOptions): string[] {
		const args: string[] = [];

		args.push("--showmeminfo", "vram");
		args.push("--json");

		if (typeof opts.device === "number") {
			args.push("--device", opts.device.toString());
		}

		return args;
	}
}
