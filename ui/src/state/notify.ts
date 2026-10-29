import { toast } from "@opendeck/design-system";

/** Shows an error toast through the design-system `Toaster` mounted in `App`. */
export function notifyError(message: string): void {
  toast({ title: message, variant: "danger" });
}

/** Shows a confirmation toast. */
export function notifySuccess(message: string): void {
  toast({ title: message, variant: "success" });
}
