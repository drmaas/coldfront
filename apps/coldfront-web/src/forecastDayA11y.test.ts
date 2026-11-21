import { describe, expect, it } from "vitest";
import { FORECAST_DAY_DETAILS_ID, dayRowButtonA11y } from "./forecastDayA11y";

describe("10-day forecast day rows", () => {
  it("uses toggle buttons tied to the detail panel, not listbox options", () => {
    expect(dayRowButtonA11y(false)).toEqual({ "aria-pressed": false });
    expect(dayRowButtonA11y(true)).toEqual({
      "aria-pressed": true,
      "aria-expanded": true,
      "aria-controls": FORECAST_DAY_DETAILS_ID,
    });
    const props = dayRowButtonA11y(true, "custom-panel");
    expect(props).not.toHaveProperty("role");
    expect(props["aria-controls"]).toBe("custom-panel");
  });
});