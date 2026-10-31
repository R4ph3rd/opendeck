import { Tabs, TabsContent, TabsList, TabsTrigger } from "@opendeck/design-system";
import { FileExplorer } from "./FileExplorer";
import { PlaylistTree } from "./PlaylistTree";

/** Library navigation (collection, playlists, sets) and the in-app file explorer. */
export function Sidebar() {
  return (
    <Tabs defaultValue="library" className="flex size-full min-h-0 min-w-0 flex-col bg-bg-subtle">
      <TabsList className="shrink-0">
        <TabsTrigger value="library">Library</TabsTrigger>
        <TabsTrigger value="files">Files</TabsTrigger>
      </TabsList>
      <TabsContent
        value="library"
        forceMount
        className="mt-0 flex min-h-0 flex-1 flex-col data-[state=inactive]:hidden"
      >
        <PlaylistTree />
      </TabsContent>
      <TabsContent
        value="files"
        forceMount
        className="mt-0 flex min-h-0 flex-1 flex-col data-[state=inactive]:hidden"
      >
        <FileExplorer />
      </TabsContent>
    </Tabs>
  );
}
