import type { Meta, StoryObj } from "@storybook/react-vite";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "./table";

const meta = { title: "Components/Table", component: Table } satisfies Meta<typeof Table>;
export default meta;
type Story = StoryObj<typeof meta>;

const rows = [
  { title: "Midnight Drive", artist: "Aria", bpm: 124, key: "8A" },
  { title: "Low Tide", artist: "Kest", bpm: 128, key: "9A" },
  { title: "Afterglow", artist: "Moa", bpm: 126, key: "8B" },
];

export const Dense: Story = {
  render: () => (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead aria-sort="ascending">Title</TableHead>
          <TableHead>Artist</TableHead>
          <TableHead className="text-right">BPM</TableHead>
          <TableHead>Key</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((r, i) => (
          <TableRow key={r.title} aria-selected={i === 1}>
            <TableCell>{r.title}</TableCell>
            <TableCell className="text-fg-muted">{r.artist}</TableCell>
            <TableCell className="text-right">{r.bpm}</TableCell>
            <TableCell className="font-mono">{r.key}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  ),
};
