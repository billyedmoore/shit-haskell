{-# LANGUAGE GADTs, TypeFamilies #-}

data family F a
data instance F [a] where
  Mk :: a -> F [a]

main :: IO ()
main = case Mk (4 :: Int) of Mk x -> print x
