{-# LANGUAGE RankNTypes #-}
type T a = forall b. a -> b -> b
main :: IO ()
main = pure ()
