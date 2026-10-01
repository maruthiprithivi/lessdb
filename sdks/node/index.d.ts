export declare class Connection {
  static open(path?: string): Connection;
  queryJson(sql: string): Promise<string>;
  queryArrow(sql: string): Promise<Buffer>;
  createTable(ddl: string): string;
  tables(): Array<string>;
  describe(table: string): string;
  optimize(table: string): Promise<string | null>;
}

export declare function open(path?: string): Connection;
