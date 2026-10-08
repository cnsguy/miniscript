(defnl transform [x]
  {:hello "hello"
   :world "world"
   :original x})

(defnl main []
  (defnl go [counter item]
    (if (< counter 100000)
      (go (+ counter 1) (transform item))
      item))
  (let [counter 0]
    (go counter {})))

(main)
(println "ok")