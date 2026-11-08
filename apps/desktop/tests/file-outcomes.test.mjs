import assert from "node:assert/strict";
import test from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { componentUrl } from "./load-component.mjs";
const { comparePlanItems, PlanPreview } = await import(await componentUrl("PlanPreview"));
const paths = ["photos/photo1.jpg", "photos/photos/photo1.jpg", "photos/ph/photo1.jpg"];
const items = paths.map((path, i) => ({ id:i+1, root_id:1, source_key:`file-${i}`, source_relative:path, destination_relative:path, hash:i===2?"different":"same", action:"copy", status:"pending", included:true, detail:null, contributors:[{root_id:1, source_relative:path}] }));
test("same contents across different paths stay separate; same name does not imply identical contents", () => {
 const evidence=comparePlanItems(items);
 assert.deepEqual(evidence.get(1).same,[paths[1]]);
 assert.deepEqual(evidence.get(2).same,[paths[0]]);
 assert.deepEqual(evidence.get(3).same,[]);
 assert.deepEqual(evidence.get(3).different,[paths[0]]);
 assert.equal(items.length,3);
});
test("photo outcomes and inclusion are visible without opening details",()=>{
 const html=renderToStaticMarkup(createElement(PlanPreview,{plan:{id:1,items,destination:"/output"},results:false,locked:false,editable:true,sourcePath:(_,p)=>`/source/${p}`,onReveal(){},onDecision(){}}));
 for(const path of paths) assert.ok(html.includes(path));
 assert.match(html,/Same contents elsewhere/);
 assert.match(html,/Same name, different contents/);
 assert.match(html,/separate locations retained/);
 assert.equal((html.match(/Copy to this location/g)||[]).length,3);
 assert.equal((html.match(/type="checkbox"/g)||[]).length,3);
 assert.doesNotMatch(html,/<details/);
 assert.doesNotMatch(html,/Exact destination|Planned action|contributing placements/);
});
test("folders and missing hash evidence never claim matching contents",()=>{
 const evidence=comparePlanItems([{...items[0],hash:null},{...items[1],action:"create_directory"},{...items[2],hash:null}]);
 assert.deepEqual(evidence.get(1),{same:[],different:[]});
 assert.deepEqual(evidence.get(3),{same:[],different:[]});
});
