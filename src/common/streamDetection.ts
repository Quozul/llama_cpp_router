export function isStreamRequest(req: unknown): boolean {
	return (
		typeof req === "object" &&
		"stream" in req &&
		typeof req.stream === "boolean" &&
		req.stream
	);
}

export function hasModelField(req: unknown): req is { model: string } {
	return (
		typeof req === "object" &&
		"model" in req &&
		typeof req.model === "string" &&
		req.model
	);
}
