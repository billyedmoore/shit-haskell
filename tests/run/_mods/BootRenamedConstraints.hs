module BootRenamedConstraints where

render :: forall b. (Eq b, Show b) => b -> String
render x
  | x == x = show x
  | otherwise = ""
