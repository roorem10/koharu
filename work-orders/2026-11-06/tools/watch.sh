# waits for the newest run on the combined branch at HEAD sha $1, then prints conclusion and annotations
SHA=$1; API=https://api.github.com/repos/roorem10/koharu
J='let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{'
while :; do
  o=$(curl -s -m 20 "$API/actions/runs?head_sha=$SHA&per_page=5" | node -e "$J"'const j=JSON.parse(s);const r=(j.workflow_runs||[]).find(x=>x.name==="Preview Windows build");console.log(r?r.id+" "+r.status+" "+(r.conclusion||""):"none")})')
  case "$o" in *completed*) echo "$o"; break;; esac
  ping -n 181 127.0.0.1 >/dev/null
done
ID=${o%% *}
curl -s -m 20 "$API/actions/runs/$ID/jobs" | node -e "$J"'for(const x of JSON.parse(s).jobs||[]){console.log("JOB",x.id,x.name,x.conclusion,x.started_at,x.completed_at);for(const t of x.steps||[])if(t.conclusion==="failure")console.log("failed step:",t.name)}})' 
JOB=$(curl -s -m 20 "$API/actions/runs/$ID/jobs" | node -e "$J"'console.log((JSON.parse(s).jobs||[])[0].id)})')
curl -s -m 20 "$API/check-runs/$JOB/annotations" | node -e "$J"'for(const a of JSON.parse(s))console.log("---",a.title||"",a.message)})'
