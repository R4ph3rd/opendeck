import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@opendeck/design-system";
import { ChevronDown, Download } from "lucide-react";
import {
  pickAndImportFolder,
  pickAndImportPlaylist,
  pickAndImportRekordbox,
} from "../state/libraryActions";

/** Top bar menu: import a folder, an M3U playlist or a rekordbox XML export. */
export function ImportMenu() {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm" variant="ghost">
          <Download className="size-3.5" aria-hidden />
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
