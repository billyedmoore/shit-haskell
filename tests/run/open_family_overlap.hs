{-# LANGUAGE TypeFamilies, DataKinds, KindSignatures, FlexibleContexts #-}
-- Compatible overlapping instances of an open family: one stuck on a rigid variable does not
-- stop a more general one from reducing (ghc-lib-parser's `Anno (StmtLR GhcRn GhcRn ..)` beside
-- `Anno (StmtLR (GhcPass pl) (GhcPass pr) ..)`). A closed family still takes its equations in order.
module Main where

data Pass = Parsed | Renamed

data GhcPass (p :: Pass) = GhcPass

type family Anno a
type instance Anno (Maybe (GhcPass 'Renamed)) = Int
type instance Anno (Maybe (GhcPass p)) = Int

type family Closed a where
  Closed (Maybe (GhcPass 'Renamed)) = Bool
  Closed (Maybe (GhcPass p)) = Int

annotate :: Maybe (GhcPass p) -> Anno (Maybe (GhcPass p))
annotate Nothing = 0
annotate (Just _) = 1

closedRenamed :: Closed (Maybe (GhcPass 'Renamed))
closedRenamed = True

closedParsed :: Closed (Maybe (GhcPass 'Parsed))
closedParsed = 3

main :: IO ()
main = do
  print (annotate (Just (GhcPass :: GhcPass 'Parsed)) + 1)
  print (closedRenamed, closedParsed)
