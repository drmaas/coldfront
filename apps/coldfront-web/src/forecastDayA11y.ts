/** Toggle buttons for the 10-day list; not a listbox. */

export const FORECAST_DAY_DETAILS_ID = "forecast-day-details";

export type DayRowButtonA11y = {
  "aria-pressed": boolean;
  "aria-expanded"?: boolean;
  "aria-controls"?: string;
};

export function dayRowButtonA11y(
  selected: boolean,
  detailsId: string = FORECAST_DAY_DETAILS_ID,
): DayRowButtonA11y {
  if (selected) {
    return {
      "aria-pressed": true,
      "aria-expanded": true,
      "aria-controls": detailsId,
    };
  }
  return { "aria-pressed": false };
}