export class Queue<T> {
  public items: T[] = [];

  constructor(initialItems: T[]) {
    this.items.push(...initialItems);
  }

  // Добавить элемент в очередь
  enqueue(item: T): void {
    this.items.push(item);
  }

  // Удалить элемент из очереди и вернуть его
  dequeue(): T | undefined {
    return this.items.shift();
  }

  // Вернуть первый элемент, не удаляя его
  peek(): T | undefined {
    return this.items[0];
  }

  // Проверить, пуста ли очередь
  isEmpty(): boolean {
    return this.items.length === 0;
  }

  // Вернуть размер очереди
  size(): number {
    return this.items.length;
  }

  // Очистить очередь
  clear(): void {
    this.items = [];
  }
}
