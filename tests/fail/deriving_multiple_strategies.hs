{-# LANGUAGE DerivingVia #-}

newtype T = T Int deriving stock Show via Int

main = pure ()
