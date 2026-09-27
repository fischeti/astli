// A label left behind used to drag the next construct into the run, and then
// the one after that.
C::c { x == 1; }
task C::t();
  y = 2;
endtask : t
task C::u();
  z = 3;
endtask : u
