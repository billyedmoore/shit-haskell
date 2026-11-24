{-# LANGUAGE TypeApplications, ExplicitForAll #-}
class C a where
  f :: forall {b} c. a -> b -> c -> (b, c)
instance C Int where
  f _ b c = (b,c)
value = f @Int @Bool (3 :: Int) 'x' True

main :: IO ()
main = print value
