{-# LANGUAGE DerivingStrategies, NoDerivingVia #-}

newtype T = T Int
deriving via Int instance Show T

main = pure ()
