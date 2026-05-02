declare module 'node:test' {
  type TestHandler = () => void | Promise<void>;

  function test(name: string, handler: TestHandler): void;

  export default test;
}

declare module 'node:assert/strict' {
  const assert: {
    deepEqual(actual: unknown, expected: unknown, message?: string): void;
    match(actual: string, expected: RegExp, message?: string): void;
    ok(value: unknown, message?: string): void;
    strictEqual(actual: unknown, expected: unknown, message?: string): void;
  };

  export default assert;
}

declare module 'node:fs' {
  export function readFileSync(path: string, options: { encoding: string } | string): string;
}
