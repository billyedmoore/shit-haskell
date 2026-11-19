-- Qualified type operators (boring's `instance a Eq.~~ b => Boring (a Eq.:~~: b)`).
{-# LANGUAGE TypeOperators, GADTs, FlexibleInstances, PolyKinds #-}
import qualified Data.Type.Equality as Eq

class Boring a where boring :: a

instance a Eq.~~ b => Boring (a Eq.:~~: b) where
  boring = Eq.HRefl

f :: (a Eq.~ Int) => a -> Int
f x = x + 1

main :: IO ()
main = case (boring :: Int Eq.:~~: Int) of Eq.HRefl -> print (f 41)
