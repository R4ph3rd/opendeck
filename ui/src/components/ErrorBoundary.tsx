import { Button, Callout } from "@opendeck/design-system";
import { Component, type ErrorInfo, type ReactNode } from "react";
import { copyDiagnostics } from "../lib/diagnostics";

interface Props {
  children: ReactNode;
  /** Reloads the UI; defaults to a page reload. Replaced in tests. */
  onReload?: () => void;
}

interface State {
  error: Error | null;
  copied: boolean;
}

/** Top-level boundary: a render error shows a recovery screen instead of a blank window. */
export class ErrorBoundary extends Component<Props, State> {
  override state: State = { error: null, copied: false };

  static getDerivedStateFromError(error: Error): State {
    return { error, copied: false };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("UI crashed", error, info.componentStack);
  }

  private reload = () => {
    if (this.props.onReload) {
      this.props.onReload();
    } else {
      window.location.reload();
    }
  };

  override render(): ReactNode {
    const { error } = this.state;
    if (!error) {
      return this.props.children;
    }
    return (
      <div
        role="alertdialog"
        aria-labelledby="crash-title"
        className="flex h-full items-center justify-center bg-bg p-6 text-fg"
      >
        <div className="flex max-w-lg flex-col gap-4">
          <h1 id="crash-title" className="text-lg font-semibold">
            OpenDeck hit a problem
          </h1>
          <Callout variant="danger" title="The interface stopped">
            <p>
              Audio keeps playing. Reload the interface to continue. If this repeats, copy the
              diagnostics and include them in a bug report.
            </p>
            <p className="font-mono text-xs text-fg-muted">{error.message}</p>
          </Callout>
          <div className="flex gap-2">
            <Button variant="primary" onClick={this.reload}>
              Reload UI
            </Button>
            <Button
              variant="ghost"
              onClick={() =>
                void copyDiagnostics({ message: error.message, stack: error.stack }).then((copied) =>
                  this.setState({ copied }),
                )
              }
            >
              {this.state.copied ? "Copied" : "Copy diagnostics"}
            </Button>
          </div>
        </div>
      </div>
    );
  }
}
