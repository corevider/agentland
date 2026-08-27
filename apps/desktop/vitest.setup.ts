const scope = globalThis as unknown as { self?: unknown; window?: unknown };

scope.self ??= globalThis;
scope.window ??= globalThis;
