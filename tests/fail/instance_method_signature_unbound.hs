{-# LANGUAGE InstanceSigs, ExplicitForAll #-}
class C a where
  method :: a -> b -> b
instance C Int where
  method :: forall b. Int -> c -> c
  method _ = id

main :: IO ()
main = pure ()
