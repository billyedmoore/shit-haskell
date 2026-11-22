{-# LANGUAGE DerivingVia, NoDerivingStrategies #-}

newtype T = T Int deriving Show via Int

main :: IO ()
main = print (T 3)
