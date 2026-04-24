export class MockChannel<T = unknown> {
  onmessage: ((data: T) => void) | undefined = undefined;

  push(data: T): void {
    this.onmessage?.(data);
  }
}
