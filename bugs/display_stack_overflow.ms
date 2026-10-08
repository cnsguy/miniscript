(defnl transform [x]
  {:hello "hello"
   :world "world"
   :original x})

(defl item {:hello "hello" :world "world"})
(defl counter 0)

(defnl main []
  (when (< counter 100000)
    (set item (transform item))
    (set counter (+ counter 1))
    (main)))

(main)
(println item)
