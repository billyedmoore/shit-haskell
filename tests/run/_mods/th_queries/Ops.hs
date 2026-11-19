module Ops ((<+>), Pair (..), plain) where

infixr 5 <+>
(<+>) :: [a] -> [a] -> [a]
(<+>) = (++)

data Pair a b = Pair a b | a :*: b
infixl 6 :*:

plain :: Int -> Int
plain = id
