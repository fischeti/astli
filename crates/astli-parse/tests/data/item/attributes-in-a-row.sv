// Attribute instances may follow one another (A.1.2, `{ attribute_instance }`),
// each its own node. Reading only the first once left the whole module to the
// fallback.
(* no_ungroup *)
(* no_boundary_optimization *)
module m;
  (* keep *) (* async_reg = 1 *) logic q;
endmodule
