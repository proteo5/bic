import chart from './img/chart.png';
import icon from './img/icon.png';
import photo from './img/photo.jpg';

// Images are created from script, so the browser test can see them as <img> elements.
for (const [id, src] of [['chart', chart], ['icon', icon], ['photo', photo]]) {
  const img = new Image();
  img.id = id;
  img.alt = id;
  img.src = src;
  document.body.append(img);
}
