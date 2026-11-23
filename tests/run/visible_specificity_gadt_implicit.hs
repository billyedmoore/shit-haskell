{-# LANGUAGE GADTs, TypeApplications #-}
data G a where
  G :: b -> c -> G c
main :: IO ()
main = case G @Char @Bool 'x' True of G _ b -> print b
