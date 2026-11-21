import { useEffect, useId, useRef, useState } from "react";
import {
  formatGeocodeLabel,
  isUsZip,
  parseGeocodeSearch,
  placeFromGeocodeHit,
  type GeocodeHit,
  type Place,
} from "../lib/domain";
import { isZipLookup, locationPath } from "../lib/location";
import {
  geocodeHitsMatchQuery,
  resolveLocationSearchFlow,
} from "../lib/locationSearchFlow";
import { writeStoredPlace } from "./locationStorage";

const SEARCH_DEBOUNCE_MS = 300;

interface LocationSearchProps {
  className?: string;
  inputClassName?: string;
  placeholder?: string;
  initialValue?: string;
  ariaLabel?: string;
  submitLabel: string;
  onNavigate: (path: string) => void;
  onError: (message: string) => void;
}

export function LocationSearch({
  className,
  inputClassName,
  placeholder = "ZIP or city",
  initialValue = "",
  ariaLabel = "Location",
  submitLabel,
  onNavigate,
  onError,
}: LocationSearchProps) {
  const [query, setQuery] = useState(initialValue);
  const [hits, setHits] = useState<GeocodeHit[]>([]);
  const [hitsQuery, setHitsQuery] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState("");
  const listId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const requestId = useRef(0);
  const queryRef = useRef(query);

  useEffect(() => {
    queryRef.current = query;
  }, [query]);

  useEffect(() => {
    setQuery(initialValue);
  }, [initialValue]);

  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed || isZipLookup(trimmed)) {
      setHits([]);
      setHitsQuery(null);
      setOpen(false);
      setSearchError("");
      setSearching(false);
      return undefined;
    }
    setHits([]);
    setHitsQuery(null);
    setOpen(false);
    setActiveIndex(-1);
    const id = requestId.current + 1;
    requestId.current = id;
    const searchQuery = trimmed;
    const handle = window.setTimeout(() => {
      setSearching(true);
      setSearchError("");
      void fetch(`/api/geocode/search?name=${encodeURIComponent(searchQuery)}`)
        .then(async (response) => {
          if (requestId.current !== id) return;
          if (queryRef.current.trim() !== searchQuery) return;
          if (response.status === 429) throw new Error("Too many city searches. Wait a moment and try again.");
          if (!response.ok) throw new Error("Could not search for that city.");
          return parseGeocodeSearch(await response.json());
        })
        .then((next) => {
          if (requestId.current !== id || !next) return;
          if (queryRef.current.trim() !== searchQuery) return;
          setHits(next);
          setHitsQuery(searchQuery);
          setOpen(next.length > 0);
          setActiveIndex(next.length > 0 ? 0 : -1);
          if (next.length === 0) setSearchError("No matching US cities found.");
        })
        .catch((reason: unknown) => {
          if (requestId.current !== id) return;
          if (queryRef.current.trim() !== searchQuery) return;
          setHits([]);
          setHitsQuery(null);
          setOpen(false);
          setSearchError(reason instanceof Error ? reason.message : "Could not search for that city.");
        })
        .finally(() => {
          if (requestId.current === id) setSearching(false);
        });
    }, SEARCH_DEBOUNCE_MS);
    return () => window.clearTimeout(handle);
  }, [query]);

  useEffect(() => {
    const onDoc = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, []);

  const choosePlace = (place: Place) => {
    writeStoredPlace(place);
    onNavigate(locationPath(place));
    setOpen(false);
    onError("");
  };

  const submit = (event: { preventDefault: () => void }) => {
    event.preventDefault();
    const trimmed = query.trim();
    const outcome = resolveLocationSearchFlow(trimmed, hits, hitsQuery);
    if (outcome.kind === "navigate") {
      if (!isUsZip(trimmed)) {
        const hit =
          geocodeHitsMatchQuery(trimmed, hitsQuery, hits) && hits.length === 1 ? hits[0] : null;
        if (hit) choosePlace(placeFromGeocodeHit(hit));
        else onNavigate(outcome.path);
      } else {
        onNavigate(outcome.path);
      }
      onError("");
      return;
    }
    if (outcome.kind === "pick") {
      setOpen(true);
      onError("Choose a city from the list.");
      return;
    }
    onError(outcome.message);
  };

  return (
    <div className={className} ref={rootRef}>
      <form onSubmit={submit} className="location-form">
        <div className="location-field">
          <input
            className={inputClassName}
            role="combobox"
            aria-haspopup="listbox"
            aria-expanded={open}
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={
              activeIndex >= 0 && hits[activeIndex] ? `${listId}-${activeIndex}` : undefined
            }
            aria-label={ariaLabel}
            autoComplete="off"
            placeholder={placeholder}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              onError("");
            }}
            onKeyDown={(event) => {
              if (!open || !geocodeHitsMatchQuery(query, hitsQuery, hits)) return;
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setActiveIndex((current) => (current + 1) % hits.length);
              } else if (event.key === "ArrowUp") {
                event.preventDefault();
                setActiveIndex((current) => (current <= 0 ? hits.length - 1 : current - 1));
              } else if (event.key === "Enter" && activeIndex >= 0) {
                event.preventDefault();
                choosePlace(placeFromGeocodeHit(hits[activeIndex]));
              } else if (event.key === "Escape") {
                setOpen(false);
              }
            }}
          />
          {open && geocodeHitsMatchQuery(query, hitsQuery, hits) ? (
            <ul className="location-listbox" id={listId} role="listbox">
              {hits.map((hit, index) => (
                <li key={`${hit.name}-${hit.latitude}-${hit.longitude}`}>
                  <button
                    type="button"
                    role="option"
                    id={`${listId}-${index}`}
                    aria-selected={index === activeIndex}
                    className={index === activeIndex ? "active" : ""}
                    onMouseEnter={() => setActiveIndex(index)}
                    onClick={() => {
                      if (!geocodeHitsMatchQuery(query, hitsQuery, hits)) return;
                      choosePlace(placeFromGeocodeHit(hit));
                    }}
                  >
                    {formatGeocodeLabel(hit)}
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
        <button className="primary" type="submit" disabled={searching}>
          {submitLabel}
        </button>
      </form>
      {searching ? <p className="form-hint">Searching cities…</p> : null}
      {searchError && !isZipLookup(query) ? <p className="form-error">{searchError}</p> : null}
    </div>
  );
}