{-# LANGUAGE DerivingStrategies, NoDerivingVia #-}

newtype T = T Int deriving Show via Int

main = pure ()
