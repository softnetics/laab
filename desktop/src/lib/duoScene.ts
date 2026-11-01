import * as THREE from "three";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";
import { drawCover, drawInnerLeft, drawInnerRight, type DuoData, loadPictures } from "./duoScreens";

// The iPhone Duo for the peek hint, in scene units: one half (and the cover
// screen) is 466 × 678 pt, so 0.687 × 1. Each half is a slab hinged on its
// edge at x = 0; the back half lies at x 0…W with its inner screen facing the
// camera, and the front half turns about the hinge, its outer face being the
// cover screen.
const W = 0.687;
const H = 1;
const T = 0.03;
const OUTER = 0.13;
const HINGE = 0.025;
const BEZEL = 0.02;
/** How far the front half opens, in degrees between the halves: a pause
 *  halfway there, then the peek. iOS keeps the inner screen on only past
 *  about 90–100° (below that it goes back to the cover screen once the hinge
 *  stops), so the peek is half open, not a crack. */
const PAUSE = 45;
const PEEK = 105;
const LOOP = 6;

/** A rounded rectangle with its hinge side (x = 0) less rounded. `side` is
 *  which way it extends from the hinge. */
function slab(side: 1 | -1, inset: number) {
  const w = W - 2 * inset;
  const h = H - 2 * inset;
  const outer = Math.max(OUTER - inset, 0.01);
  const hinge = Math.max(HINGE - inset / 2, 0.005);
  const x0 = side === 1 ? inset : -W + inset;
  const y0 = -h / 2;
  // Corners: hinge side gets `hinge`, the far side gets `outer`.
  const [rl, rr] = side === 1 ? [hinge, outer] : [outer, hinge];
  const s = new THREE.Shape();
  s.moveTo(x0 + rl, y0);
  s.lineTo(x0 + w - rr, y0);
  s.quadraticCurveTo(x0 + w, y0, x0 + w, y0 + rr);
  s.lineTo(x0 + w, y0 + h - rr);
  s.quadraticCurveTo(x0 + w, y0 + h, x0 + w - rr, y0 + h);
  s.lineTo(x0 + rl, y0 + h);
  s.quadraticCurveTo(x0, y0 + h, x0, y0 + h - rl);
  s.lineTo(x0, y0 + rl);
  s.quadraticCurveTo(x0, y0, x0 + rl, y0);
  return s;
}

/** A flat screen whose texture spans its own bounds. */
function screenGeometry(shape: THREE.Shape) {
  const g = new THREE.ShapeGeometry(shape, 24);
  g.computeBoundingBox();
  const b = g.boundingBox!;
  const pos = g.attributes.position;
  const uv = g.attributes.uv;
  for (let i = 0; i < pos.count; i++) {
    uv.setXY(i, (pos.getX(i) - b.min.x) / (b.max.x - b.min.x), (pos.getY(i) - b.min.y) / (b.max.y - b.min.y));
  }
  return g;
}

/** A screen's canvas, twice its size in points, in the screen's proportions. */
function canvas() {
  const c = document.createElement("canvas");
  c.width = 932;
  c.height = Math.round((932 * (H - 2 * BEZEL)) / (W - 2 * BEZEL));
  return c;
}

const ease = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);

const easeOut = (t: number) => 1 - Math.pow(1 - t, 3);
const easeIn = (t: number) => t * t * t;

/** Degrees between the halves over one loop, as someone peeks: shut a
 *  moment, slowly open, a pause, on to half open, reading, and shut
 *  quickly. */
function opening(t: number) {
  const s = (t % LOOP) / LOOP;
  if (s < 0.1) return 0;
  if (s < 0.32) return PAUSE * easeOut((s - 0.1) / 0.22);
  if (s < 0.4) return PAUSE;
  if (s < 0.56) return PAUSE + (PEEK - PAUSE) * ease((s - 0.4) / 0.16);
  if (s < 0.82) return PEEK;
  if (s < 0.9) return PEEK * (1 - easeIn((s - 0.82) / 0.08));
  return 0;
}

/** Draw the iPhone Duo into `el` and animate it until `stop`; `update`
 *  redraws its screens for new data. Throws when WebGL is unavailable. */
export function mountDuo(el: HTMLElement, data: DuoData, still: boolean) {
  const width = el.clientWidth;
  const height = el.clientHeight;
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: "low-power" });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 3));
  renderer.setSize(width, height);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.0;
  // Hidden until its shaders are compiled, which takes a moment the first
  // time (about a second in the simulator), so the card never shows a gap.
  const view = renderer.domElement;
  view.style.opacity = "0";
  view.style.transition = "opacity 300ms ease-out";
  el.appendChild(view);

  const scene = new THREE.Scene();
  const pmrem = new THREE.PMREMGenerator(renderer);
  const room = new RoomEnvironment();
  scene.environment = pmrem.fromScene(room, 0.04).texture;
  room.dispose();

  const camera = new THREE.PerspectiveCamera(26, width / height, 0.1, 50);
  camera.position.set(0.12, 0.46, 3.05);
  camera.lookAt(0, -0.02, 0);

  const key = new THREE.DirectionalLight(0xffffff, 1.4);
  key.position.set(-1.6, 2.4, 2.8);
  scene.add(key);
  // A rim light from behind picks the frame's edges out of a dark card.
  const rim = new THREE.DirectionalLight(0xffffff, 1.1);
  rim.position.set(2.2, 1.2, -2.4);
  scene.add(rim);

  const device = new THREE.Group();
  scene.add(device);

  const frame = new THREE.MeshPhysicalMaterial({
    color: 0x3a3a40,
    metalness: 0.85,
    roughness: 0.28,
    clearcoat: 0.8,
    clearcoatRoughness: 0.15,
  });
  const body = (side: 1 | -1) => {
    const g = new THREE.ExtrudeGeometry(slab(side, 0.006), {
      depth: T - 0.012,
      bevelEnabled: true,
      bevelThickness: 0.006,
      bevelSize: 0.006,
      bevelSegments: 5,
      curveSegments: 28,
    });
    g.translate(0, 0, -T + 0.006);
    return new THREE.Mesh(g, frame);
  };
  const textures: THREE.CanvasTexture[] = [];
  const screens: { canvas: HTMLCanvasElement; texture: THREE.CanvasTexture; draw: typeof drawCover }[] = [];
  const screen = (draw: typeof drawCover, side: 1 | -1) => {
    const c = canvas();
    const tex = new THREE.CanvasTexture(c);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.anisotropy = 4;
    textures.push(tex);
    screens.push({ canvas: c, texture: tex, draw });
    // A lit screen shows its own colours, untouched by the scene's light.
    const m = new THREE.MeshBasicMaterial({ map: tex, toneMapped: false });
    const mesh = new THREE.Mesh(screenGeometry(slab(side, BEZEL)), m);
    // The cover glass: only its reflections, added over the screen.
    const glass = new THREE.Mesh(
      new THREE.ShapeGeometry(slab(side, 0.008), 24),
      new THREE.MeshPhysicalMaterial({
        color: 0x000000,
        roughness: 0.04,
        metalness: 0,
        clearcoat: 1,
        clearcoatRoughness: 0.02,
        transparent: true,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        envMapIntensity: 0.2,
      }),
    );
    glass.position.z = 0.0015;
    mesh.add(glass);
    return mesh;
  };
  const black = new THREE.MeshPhysicalMaterial({ color: 0x050505, roughness: 0.1, clearcoat: 1 });
  const glassFace = (side: 1 | -1) => new THREE.Mesh(new THREE.ShapeGeometry(slab(side, 0.008), 24), black);

  // The back half: frame, black glass, the inner screen's right side.
  const back = new THREE.Group();
  back.add(body(1));
  const backGlass = glassFace(1);
  backGlass.position.z = 0.0005;
  back.add(backGlass);
  const innerRight = screen(drawInnerRight, 1);
  innerRight.position.z = 0.001;
  back.add(innerRight);
  device.add(back);

  // The front half, turning on the hinge: the inner screen's left side on
  // one face, the cover screen on the other.
  const front = new THREE.Group();
  front.add(body(-1));
  const frontGlass = glassFace(-1);
  frontGlass.position.z = 0.0005;
  front.add(frontGlass);
  const innerLeft = screen(drawInnerLeft, -1);
  innerLeft.position.z = 0.001;
  front.add(innerLeft);
  // The cover screen is drawn as a right-hand slab and turned over, so it
  // reads the right way round once the half is shut, hinge on its left.
  const coverGlass = glassFace(1);
  coverGlass.rotation.y = Math.PI;
  coverGlass.position.z = -T - 0.0005;
  front.add(coverGlass);
  const cover = screen(drawCover, 1);
  cover.rotation.y = Math.PI;
  cover.position.z = -T - 0.001;
  front.add(cover);
  device.add(front);

  // The spine: half a cylinder on the outside of the fold.
  const spine = new THREE.Mesh(
    new THREE.CylinderGeometry(T, T, H - 2 * HINGE, 32, 1, true, Math.PI / 2, Math.PI),
    frame,
  );
  device.add(spine);

  // A soft contact shadow under the device.
  const sc = document.createElement("canvas");
  sc.width = sc.height = 128;
  const sg = sc.getContext("2d")!;
  const grad = sg.createRadialGradient(64, 64, 4, 64, 64, 64);
  grad.addColorStop(0, "rgba(0,0,0,0.45)");
  grad.addColorStop(1, "rgba(0,0,0,0)");
  sg.fillStyle = grad;
  sg.fillRect(0, 0, 128, 128);
  const shadowTex = new THREE.CanvasTexture(sc);
  textures.push(shadowTex);
  const shadow = new THREE.Mesh(
    new THREE.PlaneGeometry(1, 1),
    new THREE.MeshBasicMaterial({ map: shadowTex, transparent: true, depthWrite: false }),
  );
  shadow.rotation.x = -Math.PI / 2;
  shadow.position.y = -H / 2 - 0.06;
  scene.add(shadow);

  const innerScreens = [innerLeft, innerRight].map((m) => m.material as THREE.MeshBasicMaterial);

  let current = data;
  const paint = () => {
    for (const s of screens) {
      s.draw(s.canvas, current);
      s.texture.needsUpdate = true;
    }
  };
  const update = (next: DuoData) => {
    current = next;
    loadPictures([next.avatar, ...next.rows.map((r) => r.avatar)], paint);
    paint();
    if (still) renderer.render(scene, camera);
  };
  update(data);

  const pose = (open: number, t: number) => {
    const p = open / PEEK;
    const turn = THREE.MathUtils.degToRad(180 - open);
    front.rotation.y = turn;
    spine.rotation.y = turn / 2;
    // Keep the device centred as it opens (the front half reaches `span`
    // left of the hinge), and let it sway a little.
    const span = W * Math.max(0, Math.cos(turn));
    device.position.x = -(W - span) / 2 + p * 0.2;
    // Turn the open side toward the camera, so it sees into the gap.
    device.rotation.y = -0.34 - p * 0.5 + Math.sin(t * 0.9) * 0.02;
    device.rotation.x = 0.1 + p * 0.06;
    device.position.y = Math.sin(t * 1.3) * 0.008;
    shadow.scale.set((W + span) * 1.25, 0.5, 1);
    // The inner screen comes on near where iOS turns it on.
    const light = THREE.MathUtils.smoothstep(open, 70, 100);
    for (const m of innerScreens) m.color.setScalar(0.03 + 0.97 * light);
  };

  let frameId = 0;
  let stopped = false;
  let start = 0;
  const tick = () => {
    const t = (performance.now() - start) / 1000;
    pose(still ? PEEK : opening(t), still ? 0 : t);
    renderer.render(scene, camera);
    if (!still && !stopped) frameId = requestAnimationFrame(tick);
  };
  renderer
    .compileAsync(scene, camera)
    .catch(() => {})
    .then(() => {
      if (stopped) return;
      start = performance.now();
      tick();
      view.style.opacity = "1";
    });

  const stop = () => {
    stopped = true;
    cancelAnimationFrame(frameId);
    scene.traverse((o) => {
      if (o instanceof THREE.Mesh) {
        o.geometry.dispose();
        const ms = Array.isArray(o.material) ? o.material : [o.material];
        ms.forEach((m) => m.dispose());
      }
    });
    textures.forEach((x) => x.dispose());
    scene.environment?.dispose();
    pmrem.dispose();
    renderer.dispose();
    renderer.domElement.remove();
  };
  return { stop, update };
}
