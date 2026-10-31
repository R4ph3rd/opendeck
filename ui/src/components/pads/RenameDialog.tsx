import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
} from "@opendeck/design-system";
import { useState, type FormEvent } from "react";

interface RenameDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  /** Name the field starts with. */
  initial: string;
  /** Called with the new name when the dialog is confirmed. */
  onCommit: (name: string) => void;
}

/** Small dialog with one name field, opened from a pad's right-click menu. */
export function RenameDialog({ open, onOpenChange, title, initial, onCommit }: RenameDialogProps) {
  const [draft, setDraft] = useState(initial);
  const [seen, setSeen] = useState(open);
  // Start from the current name each time the dialog opens.
  if (open !== seen) {
    setSeen(open);
    if (open) {
      setDraft(initial);
    }
  }
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onCommit(draft);
    onOpenChange(false);
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <form onSubmit={submit} className="flex flex-col gap-3">
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>Leave empty to clear the name.</DialogDescription>
          </DialogHeader>
          <Input
            aria-label="Name"
            autoFocus
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          />
          <DialogFooter>
            <Button type="button" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" variant="primary">
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
