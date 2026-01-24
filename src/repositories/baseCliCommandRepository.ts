import { spawn } from "node:child_process";
import { access, constants } from "node:fs/promises";
import { platform } from "node:os";
import path from "node:path";

export abstract class BaseCliCommandRepository {
	readonly binaryPath: string;

	constructor(binaryPath: string) {
		if (!binaryPath) {
			throw new Error("binaryPath must be a non‑empty string");
		}
		this.binaryPath = path.resolve(binaryPath);
	}

	async spawnAsync(
		args: string[],
	): Promise<{ stdout: string; stderr: string; exitCode: number | null }> {
		try {
			await access(this.binaryPath, constants.X_OK);
		} catch (_e) {
			throw new Error("Binary not found or not executable");
		}

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
		if (platform() === "win32") {
			return /\s/.test(arg) ? `"${arg.replace(/"/g, '\\"')}"` : arg;
		}
		return /[^\w@%+=:,./-]/.test(arg) ? `'${arg.replace(/'/g, "'\\''")}'` : arg;
	}
}
