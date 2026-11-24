{-# LANGUAGE StandaloneKindSignatures,TypeFamilies,PolyKinds,TypeOperators,TypeAbstractions #-}
import Data.Kind (Type)
type family F _ a where
  F x a = a
main :: IO ()
main = print (3 :: F Bool Int)
