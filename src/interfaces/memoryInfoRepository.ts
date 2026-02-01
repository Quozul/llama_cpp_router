export type MemoryUsageInfo = {
	sourceId: string; // e.g., "card0" or "system"
	totalBytes: number;
	usedBytes: number;
	freeBytes: number;
};

export interface MemoryInfoRepository {
	getMemoryInfo(): Promise<MemoryUsageInfo[]>;
}
