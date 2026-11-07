// Private inherited CDP pipes only. Never log requests, replies, errors or secret values.
const {spawn}=require('node:child_process')
const {once}=require('node:events')
const [executable,folder,...flags]=process.argv.slice(1)
if(!executable||!folder)process.exit(1)
const proxy=flags.find(f=>f.startsWith('--proxy='))?.slice('--proxy='.length)
const probe=flags.includes('--headless-probe')
const browser=spawn(executable,[`--user-data-dir=${folder}`,'--remote-debugging-pipe',...(proxy?[`--proxy-server=${proxy}`]:[]),...(probe?['--headless=new']:[]),'--incognito','--no-first-run','--no-default-browser-check','--disable-extensions','--disable-sync','--new-window','about:blank'],{stdio:['ignore','ignore','ignore','pipe','pipe'],windowsHide:!!probe})
const limit=262144
const requests=new Set()
let lastId=0
let stopping=false
function stop(){if(stopping)return;stopping=true;browser.kill();process.exit(1)}
browser.on('error',stop)
browser.on('exit',()=>process.exit(0))
process.stdin.on('end',stop)
process.stdin.on('error',stop)
process.stdout.on('error',stop)
async function pump(source,delimiter,receive){
  let buffered=Buffer.alloc(0)
  for await(const chunk of source){
    buffered=Buffer.concat([buffered,chunk])
    let index
    while((index=buffered.indexOf(delimiter))>=0){
      if(index>limit)throw Error('limit')
      const message=JSON.parse(buffered.subarray(0,index).toString('utf8'))
      buffered=buffered.subarray(index+1)
      await receive(message)
    }
    if(buffered.length>limit)throw Error('limit')
  }
}
async function write(stream,message,delimiter){
  const encoded=Buffer.from(JSON.stringify(message)+delimiter,'utf8')
  if(encoded.length>limit)throw Error('limit')
  if(!stream.write(encoded))await once(stream,'drain')
}
Promise.all([
  pump(process.stdin,10,async message=>{
    if(!Number.isSafeInteger(message.id)||message.id<=lastId||typeof message.method!=='string')throw Error('identity')
    lastId=message.id;requests.add(message.id)
    await write(browser.stdio[3],message,'\0')
  }),
  pump(browser.stdio[4],0,async message=>{
    // Console/network events from pages must never enter the host receipt/error path.
    if(!requests.delete(message.id))return
    await write(process.stdout,message,'\n')
  }),
]).then(stop,stop)
