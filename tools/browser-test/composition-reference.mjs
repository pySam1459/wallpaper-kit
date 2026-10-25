import { effectReference } from './effect-reference.mjs';

// Independently rasterize the input scene and composition rectangle. Only the
// effect itself runs the installed GLSL; no native captures feed this reference.
export async function compositionReference(page, sources, images, fixture, time, chain = fixture.chain, width = images.width, height = images.height) {
  const w = images.width, h = images.height;
  const original = Buffer.from(fixture.rgba, 'base64');
  const canvas = new Uint8Array(width * height * 4);
  const sample = (data, sw, sh, x, y) => {
    const left = Math.floor(x), top = Math.floor(y), fx = x-left, fy = y-top;
    const pixel = (x,y,c) => data[(Math.max(0,Math.min(sh-1,y))*sw+Math.max(0,Math.min(sw-1,x)))*4+c];
    return [0,1,2,3].map(c => (pixel(left,top,c)*(1-fx)+pixel(left+1,top,c)*fx)*(1-fy)
      +(pixel(left,top+1,c)*(1-fx)+pixel(left+1,top+1,c)*fx)*fy);
  };
  for (let y=0;y<height;y++) for (let x=0;x<width;x++) {
    const color = sample(original,w,h,(x+.5)*w/width-.5,(y+.5)*h/height-.5);
    const alpha = color[3]/255 * fixture.image_opacity;
    const at=(y*width+x)*4;
    for(let c=0;c<3;c++) canvas[at+c]=Math.round(color[c]*alpha);
    canvas[at+3]=255;
  }
  const overlay = spec => {
    const [left,top,rw,rh,color] = spec;
    for(let y=0;y<height;y++) for(let x=0;x<width;x++) {
      const u=(x+.5)*w/width, v=(y+.5)*h/height;
      if(u>=left&&u<left+rw&&v>=top&&v<top+rh) canvas.set(color,(y*width+x)*4);
    }
  };
  overlay(fixture.before);
  const vw=Math.ceil(fixture.size[0]/w*width), vh=Math.ceil(fixture.size[1]/h*height);
  const cw=fixture.full ? width : Math.ceil(fixture.size[0]);
  const ch=fixture.full ? height : Math.ceil(fixture.size[1]);
  const [cos,sin] = fixture.angle === 90 ? [0,1] : [1,0];
  const project = (x,y,rw,rh) => {
    const u=(x+.5)/rw*fixture.size[0]-fixture.size[0]/2;
    const v=(y+.5)/rh*fixture.size[1]-fixture.size[1]/2;
    return [(fixture.center[0]+cos*u+sin*v)/w*width,(fixture.center[1]-sin*u+cos*v)/h*height];
  };
  const capture = new Uint8Array(cw*ch*4);
  for(let y=0;y<ch;y++) for(let x=0;x<cw;x++) {
    const [sx,sy]=project(x,y,cw,ch);
    capture.set(sample(canvas,width,height,sx-.5,sy-.5).map(Math.round),(y*cw+x)*4);
  }
  let rgba=Buffer.from(capture).toString('base64'), result;
  for(const [index,pass] of chain.entries()) {
    const final = index+1===chain.length;
    result=await page.evaluate(effectReference,{...sources[pass.effect],...images,...fixture,...pass,
      width:final?vw:cw,height:final?vh:ch,source_width:cw,source_height:ch,rgba,time,raw:true});
    rgba=Buffer.from(result).toString('base64');
  }
  for(let y=0;y<vh;y++) for(let x=0;x<vw;x++) {
    const [px,py]=project(x,y,vw,vh), dx=Math.floor(px),dy=Math.floor(py);
    if(dx<0||dx>=width||dy<0||dy>=height) continue;
    const at=(dy*width+dx)*4, src=(y*vw+x)*4, alpha=fixture.opacity??1;
    for(let c=0;c<3;c++) canvas[at+c]=Math.round(Math.min(255,result[src+c]*(fixture.brightness??1))*alpha+canvas[at+c]*(1-alpha));
  }
  overlay(fixture.after);
  return [...canvas];
}
