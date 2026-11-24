{-# LANGUAGE InstanceSigs, TypeFamilies #-}
type family F a
type family G a
class C a where
  method :: a -> F a
instance C [a] where
  method :: [a] -> G [a]
  method _ = error "unused"

main :: IO ()
main = pure ()
