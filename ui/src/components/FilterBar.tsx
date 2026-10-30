import {
  Button,
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@opendeck/design-system";
import { Columns3, Search, X } from "lucide-react";
import {
  CAMELOT_KEYS,
  COLUMN_LABEL,
  COLUMN_ORDER,
  hasFilters,
} from "../lib/library";
import { useLibraryStore } from "../state/library";
import { MAX_STARS } from "./StarRating";
import { REQUIRED_COLUMNS } from "./trackColumns";

const ANY = "any";
const RATINGS = Array.from({ length: MAX_STARS }, (_, i) => i + 1);

const toNumber = (text: string): number | null => {
  const value = Number.parseFloat(text);
  return Number.isFinite(value) ? value : null;
};

interface FilterBarProps {
  /** Hide the filters a fixed list cannot apply (only text search is left). */
  textOnly?: boolean;
}

/** Search box, BPM, key, genre and rating filters, key notation and column menu. */
export function FilterBar({ textOnly = false }: FilterBarProps) {
  const filters = useLibraryStore((s) => s.filters);
  const setFilters = useLibraryStore((s) => s.setFilters);
  const clearFilters = useLibraryStore((s) => s.clearFilters);
  const notation = useLibraryStore((s) => s.keyNotation);
  const setNotation = useLibraryStore((s) => s.setKeyNotation);
  const visibility = useLibraryStore((s) => s.columnVisibility);
  const setVisibility = useLibraryStore((s) => s.setColumnVisibility);

  return (
    <div
      role="search"
      className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-3 py-2"
    >
      <div className="relative w-56">
        <Search
          className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-fg-subtle"
          aria-hidden
        />
        <Input
          type="search"
          aria-label="Search collection"
          placeholder="Search title, artist, album"
          className="pl-7"
          value={filters.text}
          onChange={(event) => setFilters({ text: event.target.value })}
        />
      </div>
      {textOnly ? null : (
        <>
          <Input
            type="number"
            aria-label="Minimum BPM"
            placeholder="BPM min"
            className="w-20"
            value={filters.bpmMin ?? ""}
            onChange={(event) => setFilters({ bpmMin: toNumber(event.target.value) })}
          />
          <Input
            type="number"
            aria-label="Maximum BPM"
            placeholder="BPM max"
            className="w-20"
            value={filters.bpmMax ?? ""}
            onChange={(event) => setFilters({ bpmMax: toNumber(event.target.value) })}
          />
          <Select
            value={filters.key ?? ANY}
            onValueChange={(value) => setFilters({ key: value === ANY ? null : value })}
          >
            <SelectTrigger aria-label="Key filter" className="w-24">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ANY}>Any key</SelectItem>
              {CAMELOT_KEYS.map((key) => (
                <SelectItem key={key} value={key}>
                  {key}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Input
            aria-label="Genre filter"
            placeholder="Genre"
            className="w-28"
            value={filters.genre}
            onChange={(event) => setFilters({ genre: event.target.value })}
          />
          <Select
            value={String(filters.minRating)}
            onValueChange={(value) => setFilters({ minRating: Number(value) })}
          >
            <SelectTrigger aria-label="Rating filter" className="w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="0">Any rating</SelectItem>
              {RATINGS.map((stars) => (
                <SelectItem key={stars} value={String(stars)}>
                  {stars}+ {stars === 1 ? "star" : "stars"}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </>
      )}
      {hasFilters(filters) ? (
        <Button size="sm" variant="ghost" onClick={clearFilters}>
          <X className="size-3" aria-hidden />
          Clear
        </Button>
      ) : null}
      <div className="flex-1" />
      <Button
        size="sm"
        variant="ghost"
        aria-pressed={notation === "musical"}
        aria-label={`Key notation: ${notation === "camelot" ? "Camelot" : "musical"}. Switch`}
        onClick={() => setNotation(notation === "camelot" ? "musical" : "camelot")}
      >
        {notation === "camelot" ? "Camelot" : "Musical"}
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button size="sm" variant="ghost" aria-label="Choose columns">
            <Columns3 className="size-3.5" aria-hidden />
            Columns
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuLabel>Show columns</DropdownMenuLabel>
          <DropdownMenuSeparator />
          {COLUMN_ORDER.map((id) => (
            <DropdownMenuCheckboxItem
              key={id}
              checked={visibility[id] !== false}
              disabled={REQUIRED_COLUMNS.has(id)}
              onSelect={(event) => event.preventDefault()}
              onCheckedChange={(checked) => setVisibility({ ...visibility, [id]: checked })}
            >
              {COLUMN_LABEL[id]}
            </DropdownMenuCheckboxItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
