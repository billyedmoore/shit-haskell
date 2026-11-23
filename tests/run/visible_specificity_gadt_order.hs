{-# LANGUAGE GADTs, TypeApplications #-}
data G a where
  G :: forall b a. b -> a -> G a
main :: IO ()
main = case G @Char @Bool 'x' True of G _ b -> print b
