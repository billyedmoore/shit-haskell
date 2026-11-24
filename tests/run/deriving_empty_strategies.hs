{-# LANGUAGE DerivingStrategies, NoDeriveAnyClass, NoGeneralisedNewtypeDeriving #-}

data T = T deriving anyclass ()
newtype N = N Int deriving newtype ()

main :: IO ()
main = print True
