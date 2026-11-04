import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@opendeck/design-system";
import { ChevronDown } from "lucide-react";
import {
  pickAndImportFolder,
  pickAndImportPlaylist,
  pickAndImportRekordbox,
} from "../state/libraryActions";

/** Menu row entry: import a folder, an M3U playlist or a rekordbox XML export. */
export function ImportMenu() {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          size="sm"
          variant="ghost"
          className="h-5 gap-1 rounded-sm bg-surface-raised px-2 text-xs font-bold uppercase hover:bg-surface-hover"
        >
          Import
          <ChevronDown className="size-3" aria-hidden />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={() => void pickAndImportFolder()}>Import folder</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => void pickAndImportPlaylist()}>
          Import playlist (M3U)
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => void pickAndImportRekordbox()}>
          Import rekordbox XML
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
