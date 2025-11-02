import { type ChildProcessByStdio, spawn } from "node:child_process";
import * as os from "node:os";
import * as path from "node:path";
import type { Readable } from "node:stream";
import type { ModelConfiguration } from "#src/repositories/configRepository.ts";

/* --------------------------- Public Types --------------------------- */

/**
 * Options required to launch a llama‑server instance.
 */
export type LlamaServerLaunchOptions = {
	/** Model name passed to `-m` (e.g. "mistral-7b") */
	modelFilePath: string;
	/** IP address to bind to – passed to `--host` */
	ipAddress: string;
	/** TCP port – passed to `--port` */
	port: number;
};

/**
 * Result of a successful `start()` call.
 */
export type LlamaServerHandle = {
	/** PID of the spawned process */
	pid: number;
};

/* --------------------------- Custom Errors --------------------------- */

/**
 * Thrown when the llama‑server binary cannot be started or exits before it
 * reports that it is listening.
 */
export class LlamaServerStartError extends Error {
	/** The full command line that was executed */
	public readonly command: string;
	/** Anything that the child wrote to stderr */
	public readonly stderr: string;

	constructor(message: string, command: string, stderr: string) {
		super(message);
		this.name = "LlamaServerStartError";
		this.command = command;
		this.stderr = stderr;
	}
}

/**
 * Thrown when stopping a server fails (e.g. unknown pid, or the process
 * cannot be terminated even after the forced kill).
 */
export class LlamaServerStopError extends Error {
	/** The pid we tried to stop */
	public readonly pid: number;

	constructor(message: string, pid: number) {
		super(message);
		this.name = "LlamaServerStopError";
		this.pid = pid;
	}
}

type ChildProcessWithoutStdin = ChildProcessByStdio<null, Readable, Readable>;

export class LlamaServerRepository {
	readonly #binaryPath: string;

	readonly #processes = new Map<number, ChildProcessWithoutStdin>();

	constructor(binaryPath: string) {
		if (!binaryPath) {
			throw new Error("binaryPath must be supplied to LlamaServerRepository");
		}
		this.#binaryPath = path.resolve(binaryPath);
	}

	public async start(opts: ModelConfiguration): Promise<LlamaServerHandle> {
		const args = this.#buildArgs(opts);
		const commandStr = `${this.#binaryPath} ${args
			.map(this.#escapeArg)
			.join(" ")}`;

		const child: ChildProcessWithoutStdin = spawn(this.#binaryPath, args, {
			stdio: ["ignore", "pipe", "pipe"],
		});

		if (child.pid === undefined) {
			throw new LlamaServerStartError(
				`llama‑server failed to spawn`,
				commandStr,
				child.stderr.readable ? "" : "",
			);
		}

		this.#processes.set(child.pid, child);

		let stdoutBuffer = "";

		const readyPromise = new Promise<void>((resolve, reject) => {
			const onData = (chunk: string) => {
				stdoutBuffer += chunk;
				const lines = stdoutBuffer.split(/\r?\n/);
				for (const line of lines) {
					if (line.includes("main: server is listening on")) {
						child.stdout.off("data", onData);
						child.off("exit", onExit);
						resolve();
						return;
					}
				}
				if (!stdoutBuffer.endsWith("\n")) {
					stdoutBuffer = lines[lines.length - 1];
				} else {
					stdoutBuffer = "";
				}
			};

			const onExit = (code: number | null, signal: string | null) => {
				child.stdout.off("data", onData);
				reject(
					new LlamaServerStartError(
						`llama‑server exited before reporting that it was listening (code=${code}, signal=${signal})`,
						commandStr,
						child.stderr.readable ? "" : "",
					),
				);
			};

			child.stderr.setEncoding("utf8");
			child.stderr.on("data", onData);
			child.on("exit", onExit);
		});

		let stderr = "";
		child.stderr.setEncoding("utf8");
		child.stderr.on("data", (chunk) => {
			stderr += chunk;
		});

		await readyPromise;

		return { pid: child.pid };
	}

	public async stop(pid: number): Promise<void> {
		const child = this.#processes.get(pid);
		if (!child) {
			throw new LlamaServerStopError(
				`No known llama‑server process with pid ${pid}`,
				pid,
			);
		}

		const waitForExit = new Promise<void>((resolve) => {
			const onClose = () => {
				clearTimeout(killTimeout);
				resolve();
			};
			child.once("close", onClose);
		});

		child.kill(); // default = SIGTERM

		const killTimeout = setTimeout(() => {
			if (!child.killed) {
				try {
					child.kill("SIGKILL");
				} catch {}
			}
		}, 30_000);

		await waitForExit;

		this.#processes.delete(pid);
	}

	#buildArgs(opts: ModelConfiguration): string[] {
		const {
			modelFilePath,
			multimodalProjectorFilePath,
			common,
			network,
			sampling,
		} = opts;
		const args: string[] = [];

		args.push("-m", modelFilePath);
		if (multimodalProjectorFilePath) {
			args.push("--mmproj", multimodalProjectorFilePath);
		}

		// network
		args.push("--host", network.host);
		args.push("--port", network.port.toString());

		// common
		args.push("--flash-attn", common.flashAttention ? "on" : "off");
		args.push("--cache-type-v", common.cacheType);
		args.push("--cache-type-k", common.cacheType);
		args.push("--ctx-size", common.contextSize.toString());
		args.push("--threads", common.threads.toString());
		args.push("--n-gpu-layers", common.nGpuLayers.toString());
		if (common.noMmap) {
			args.push("--no-mmap");
		}
		if (common.jinja) {
			args.push("--jinja");
		}

		// sampling
		args.push("--temp", sampling.temperature.toString());
		args.push("--top-k", sampling.topK.toString());
		args.push("--top-p", sampling.topP.toString());
		args.push("--min-p", sampling.minP.toString());
		args.push("--repeat-penalty", sampling.repeatPenalty.toString());
		args.push("--mirostat", sampling.mirostat.toString());

		return args;
	}

	#escapeArg(arg: string): string {
		if (os.platform() === "win32") {
			return /\s/.test(arg) ? `"${arg.replace(/"/g, '\\"')}"` : arg;
		}
		return /[^\w@%+=:,./-]/.test(arg) ? `'${arg.replace(/'/g, "'\\''")}'` : arg;
	}
}
