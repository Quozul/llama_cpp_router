import * as os from "node:os";
import type {
	MemoryInfoRepository,
	MemoryUsageInfo,
} from "#src/interfaces/memoryInfoRepository.ts";

export class SystemMemoryRepository implements MemoryInfoRepository {
	public async getMemoryInfo(): Promise<MemoryUsageInfo[]> {
		const total = os.totalmem();
		const free = os.freemem();
		const used = total - free;

		return [
			{
				sourceId: "system",
				totalBytes: total,
				usedBytes: used,
				freeBytes: free,
			},
		];
	}
}
