import { ArrowRight, Check } from "lucide-react";
import type { Comparison, Snapshot } from "./api";

type Props = {
  locations: Comparison[];
  roots: Snapshot["roots"];
  rootName: (id: number) => string;
  currentKey: string;
  currentLabel: string;
  onReveal: (key: string) => void;
};

export function MatchingLocationList({ locations, roots, rootName, currentKey, currentLabel, onReveal }: Props) {
  return <ul className="matching-location-list">{locations.map(location => {
    const current = location.key === currentKey;
    const rootPath = roots.find(root => root.id === location.root_id)?.path;
    return <li key={location.key}><button className={`placement-link ${current ? "current-location" : ""}`} aria-current={current ? "true" : undefined} aria-label={`Show in tree: ${rootName(location.root_id)}, ${location.relative_path}${current ? ` (${currentLabel})` : ""}`} onClick={() => onReveal(location.key)}><span><strong>{rootName(location.root_id)}{current && <em>{currentLabel}</em>}</strong><small>{location.relative_path}</small><small className="location-root" title={rootPath}>Root: {rootPath}</small></span><span className="location-action">Show in tree {current ? <Check size={14} /> : <ArrowRight size={14} />}</span></button></li>;
  })}</ul>;
}
