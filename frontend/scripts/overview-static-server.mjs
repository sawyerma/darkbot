// Static test fixture only. No proxies, cookies, engine access or production routes.
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
const root=path.resolve("dist");
http.createServer((req,res)=>{
  const url=new URL(req.url,"http://127.0.0.1");
  if(url.pathname.startsWith("/api/")||url.pathname.startsWith("/ws/")){res.writeHead(503);res.end();return;}
  let file=path.resolve(root,"."+url.pathname);
  if(!file.startsWith(root+path.sep)||!fs.existsSync(file)||!fs.statSync(file).isFile())file=path.join(root,"index.html");
  const type=({".html":"text/html",".js":"text/javascript",".css":"text/css"})[path.extname(file)]||"application/octet-stream";
  res.writeHead(200,{"Content-Type":type});fs.createReadStream(file).pipe(res);
}).listen(4188,"127.0.0.1");
