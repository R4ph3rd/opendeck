import { cn } from "@opendeck/design-system";
import { Star } from "lucide-react";

/** Stars a track can have. */
export const MAX_STARS = 5;

const STARS = Array.from({ length: MAX_STARS }, (_, i) => i + 1);

interface StarRatingProps {
  value: number;
  /** Called with the new rating; clicking the current rating clears it. */
  onChange?: (stars: number) => void;
  label: string;
  className?: string;
}

/** Five stars, editable when `onChange` is given. Clicks do not select the row behind. */
export function StarRating({ value, onChange, label, className }: StarRatingProps) {
  return (
    <span
      role="group"
      aria-label={label}
      className={cn("inline-flex items-center", className)}
      onClick={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
    >
      {STARS.map((star) => {
        const filled = star <= value;
        const icon = (
          <Star
            className={cn("size-3", filled ? "fill-current text-warning" : "text-fg-subtle")}
            aria-hidden
          />
        );
        return onChange ? (
          <button
            key={star}
            type="button"
            aria-label={`${star} ${star === 1 ? "star" : "stars"}`}
            aria-pressed={star === value}
            className="rounded-sm p-px hover:text-fg focus-visible:outline-2 focus-visible:outline-focus-ring"
            onClick={() => onChange(star === value ? 0 : star)}
          >
            {icon}
          </button>
        ) : (
          <span key={star} aria-hidden>
            {icon}
          </span>
        );
      })}
    </span>
  );
}
