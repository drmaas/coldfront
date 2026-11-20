import { describe, expect, it } from "vitest";
import { buildEchoField, denoiseRadar, sampleEchoes } from "./radarRelief";

function imageData(
  width: number,
  height: number,
  fill: (x: number, y: number, pixels: Uint8ClampedArray, i: number) => void,
): ImageData {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      fill(x, y, data, (y * width + x) * 4);
    }
  }
  return { data, width, height, colorSpace: "srgb" } as ImageData;
}

describe("sampleEchoes", () => {
  it("drops transparent cells", () => {
    const image = imageData(4, 4, (_x, _y, pixels, i) => {
      pixels[i] = 0;
      pixels[i + 1] = 255;
      pixels[i + 2] = 0;
      pixels[i + 3] = 0;
    });
    expect(sampleEchoes(image, 2, 2)).toEqual([]);
  });

  it("drops pale basemap / stage pixels", () => {
    const image = imageData(4, 4, (_x, _y, pixels, i) => {
      pixels[i] = 213;
      pixels[i + 1] = 227;
      pixels[i + 2] = 236;
      pixels[i + 3] = 255;
    });
    expect(sampleEchoes(image, 2, 2)).toEqual([]);
  });

  it("makes green shorter than yellow shorter than red", () => {
    const image = imageData(12, 4, (x, _y, pixels, i) => {
      if (x < 4) {
        pixels[i] = 0;
        pixels[i + 1] = 200;
        pixels[i + 2] = 0;
      } else if (x < 8) {
        pixels[i] = 245;
        pixels[i + 1] = 230;
        pixels[i + 2] = 0;
      } else {
        pixels[i] = 230;
        pixels[i + 1] = 0;
        pixels[i + 2] = 0;
      }
      pixels[i + 3] = 255;
    });
    const echoes = sampleEchoes(image, 3, 1);
    expect(echoes).toHaveLength(3);
    const green = echoes.find((echo) => echo.u < 1 / 3)!;
    const yellow = echoes.find((echo) => echo.u > 1 / 3 && echo.u < 2 / 3)!;
    const red = echoes.find((echo) => echo.u > 2 / 3)!;
    expect(green.height).toBeLessThan(yellow.height);
    expect(yellow.height).toBeLessThan(red.height);
    expect(green.g).toBeGreaterThan(green.r);
    expect(yellow.r).toBeGreaterThan(0);
    expect(yellow.g).toBeGreaterThan(0);
    expect(red.r).toBeGreaterThan(red.g);
  });

  it("keeps grid coordinates inside 0–1", () => {
    const image = imageData(6, 6, (_x, _y, pixels, i) => {
      pixels[i] = 255;
      pixels[i + 1] = 0;
      pixels[i + 2] = 0;
      pixels[i + 3] = 255;
    });
    for (const echo of sampleEchoes(image, 3, 3)) {
      expect(echo.u).toBeGreaterThan(0);
      expect(echo.u).toBeLessThan(1);
      expect(echo.v).toBeGreaterThan(0);
      expect(echo.v).toBeLessThan(1);
    }
  });

  it("keeps anti-aliased IEM greens", () => {
    const image = imageData(4, 4, (_x, _y, pixels, i) => {
      pixels[i] = 53;
      pixels[i + 1] = 214;
      pixels[i + 2] = 91;
      pixels[i + 3] = 255;
    });
    const echoes = sampleEchoes(image, 2, 2);
    expect(echoes.length).toBeGreaterThan(0);
    expect(echoes[0]!.height).toBeGreaterThan(0);
  });

  it("drops isolated speckle and keeps a storm cluster", () => {
    const image = imageData(6, 6, (x, y, pixels, i) => {
      const speck = x === 0 && y === 0;
      const block = x >= 3 && y >= 3;
      if (!speck && !block) return;
      pixels[i] = 0;
      pixels[i + 1] = 200;
      pixels[i + 2] = 0;
      pixels[i + 3] = 255;
    });
    denoiseRadar(image);
    expect(image.data[3]).toBe(0);
    expect(image.data[(3 * 6 + 3) * 4 + 3]).toBe(255);
    expect(sampleEchoes(image, 3, 3).length).toBeGreaterThan(0);
  });

  it("drops near-gray noise", () => {
    const image = imageData(4, 4, (_x, _y, pixels, i) => {
      pixels[i] = 154;
      pixels[i + 1] = 162;
      pixels[i + 2] = 180;
      pixels[i + 3] = 255;
    });
    expect(sampleEchoes(image, 2, 2)).toEqual([]);
  });

  it("builds a height field from echoes", () => {
    const echoes = sampleEchoes(
      imageData(8, 4, (x, _y, pixels, i) => {
        pixels[i] = x < 4 ? 0 : 230;
        pixels[i + 1] = x < 4 ? 200 : 0;
        pixels[i + 2] = 0;
        pixels[i + 3] = 255;
      }),
      4,
      2,
    );
    const field = buildEchoField(echoes, 4, 2);
    expect(field.height.length).toBe(8);
    expect(Math.max(...field.height)).toBeGreaterThan(0.2);
  });
});
