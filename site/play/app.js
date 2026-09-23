/*
 * The bundled demo app, exactly as examples/index.html ships it. Its script is a separate
 * file only because the site's CSP allows no inline script, and an app that cannot run is a
 * poor thing to demonstrate a recorder against.
 */
// Deliberately trivial. The point of this page is to give kaviri something with a text field
// and a button, so a recording of it exercises the typing pan and the framing.
const list = document.getElementById("list");
const empty = document.getElementById("empty");
const q = document.getElementById("q");

function add(text){
  empty.hidden = true;
  const li = document.createElement("li");
  const id = "PX-" + Math.floor(100000 + Math.random() * 899999);
  li.innerHTML = '<span class="id"></span><span class="where"></span><span class="tag">In transit</span>';
  li.querySelector(".id").textContent = id;
  li.querySelector(".where").textContent = text;
  list.prepend(li);
}

document.getElementById("go").onclick = () => {
  const v = q.value.trim();
  if(!v) return;
  add(v);
  q.value = "";
  q.focus();
};
q.addEventListener("keydown", e => {
  if(e.key === "Enter"){ e.preventDefault(); document.getElementById("go").click(); }
});
