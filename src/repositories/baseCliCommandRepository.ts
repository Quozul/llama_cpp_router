import { spawn } from "node:child_process";
import * as os from "node:os";
import path from "node:path";

export abstract class BaseCliCommandRepository {
	readonly binaryPath: string;

	constructor(binaryPath: string) {
		if (!binaryPath) {
			throw new Error("binaryPath must be a non‑empty string");
		}
		this.binaryPath = path.resolve(binaryPath);
	}

	spawnAsync(
		args: string[],
	): Promise<{ stdout: string; stderr: string; exitCode: number | null }> {
		return new Promise((resolve) => {
			const child = spawn(this.binaryPath, args, {
				stdio: ["ignore", "pipe", "pipe"],
			});

			let stdout = "";
			let stderr = "";

			child.stdout.setEncoding("utf8");
			child.stderr.setEncoding("utf8");

			child.stdout.on("data", (chunk) => {
				stdout += chunk;
			});
			child.stderr.on("data", (chunk) => {
				stderr += chunk;
			});

			child.on("close", (code) => {
				resolve({ stdout, stderr, exitCode: code });
			});
		});
	}

	escapeArg(arg: string): string {
		if (os.platform() === "win32") {
			return /\s/.test(arg) ? `"${arg.replace(/"/g, '\\"')}"` : arg;
		}
		return /[^\w@%+=:,./-]/.test(arg) ? `'${arg.replace(/'/g, "'\\''")}'` : arg;
	}
}
