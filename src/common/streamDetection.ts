export function isStreamRequest(req: unknown): boolean {
	return (
		typeof req === "object" &&
		req !== null &&
		"stream" in req &&
		typeof req.stream === "boolean" &&
		req.stream
	);
}

export function hasModelField(req: unknown): req is { model: string } {
	return (
		typeof req === "object" &&
		req !== null &&
		"model" in req &&
		typeof req.model === "string"
	);
}
