{-# LANGUAGE ExplicitForAll #-}
class C a where
  m :: forall b. a -> missing -> b
main :: IO ()
main = pure ()
