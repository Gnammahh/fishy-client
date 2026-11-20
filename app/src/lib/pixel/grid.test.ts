import { describe, it, expect } from "vitest";
import { parseGrid, gridToRects } from "./grid";

describe("grid", () => {
  it("parseGrid strips row numbers and blank lines", () => {
    expect(parseGrid("\n 0 .#.\n 1 ##.\n")).toEqual([".#.", "##."]);
  });

  it("gridToRects maps palette chars only", () => {
    expect(gridToRects([".#", "o?"], { "#": "#4FE3D1", o: "#FF9F43" })).toEqual([
      { x: 1, y: 0, color: "#4FE3D1" },
      { x: 0, y: 1, color: "#FF9F43" },
    ]);
  });
});
